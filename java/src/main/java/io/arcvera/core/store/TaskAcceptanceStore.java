package io.arcvera.core.store;

import io.arcvera.core.lease.InputLeaseStore;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.util.Objects;
import java.util.UUID;
import org.springframework.dao.DataIntegrityViolationException;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Service;
import org.springframework.transaction.support.TransactionSynchronizationManager;

/** Caller must provide authenticated principal and the configured shared storage components. */
@Service
public final class TaskAcceptanceStore {
    public record Retention(Duration maxAge, Duration futureTolerance, Duration grace, Duration retention) {
        public Retention {
            Objects.requireNonNull(maxAge); Objects.requireNonNull(futureTolerance); Objects.requireNonNull(grace); Objects.requireNonNull(retention);
            if (maxAge.isNegative() || maxAge.isZero() || futureTolerance.isNegative() || grace.isNegative()
                    || retention.compareTo(maxAge.plus(grace)) < 0) throw new IllegalArgumentException("invalid retention");
        }
    }
    public record Request(String principal, UUID operationId, String operation, Instant createdAt,
                          String payloadDigest, UUID conversationId, long expectedRevision, InputLeaseStore.Proof lease) {
        public Request {
            Objects.requireNonNull(principal); Objects.requireNonNull(operationId); Objects.requireNonNull(operation);
            Objects.requireNonNull(createdAt); Objects.requireNonNull(payloadDigest); Objects.requireNonNull(conversationId); Objects.requireNonNull(lease);
            if (principal.isBlank() || operation.isBlank() || payloadDigest.isBlank() || expectedRevision < 0
                    || createdAt.getNano() % 1_000_000 != 0) throw new IllegalArgumentException("invalid request");
        }
    }
    public record Receipt(UUID taskId, UUID conversationId, long revision, long sequence) { }
    public record Snapshot(UUID conversationId, long revision, long sequence, UUID activeTaskId, String activeState) { }
    public static final class Conflict extends RuntimeException {
        private static final long serialVersionUID = 1L;
        public Conflict(String message) { super(message); }
    }
    private final JdbcClient jdbc;
    private final ConversationRepository conversations;
    private final ReceiptRepository receipts;
    private final AcceptanceTransaction acceptance;
    private final OperationIdentityService identities;
    private final Clock clock;
    private final Retention policy;
    TaskAcceptanceStore(JdbcClient jdbc, ConversationRepository conversations, ReceiptRepository receipts,
                        AcceptanceTransaction acceptance, OperationIdentityService identities, Clock clock, Retention policy) {
        this.jdbc = Objects.requireNonNull(jdbc); this.conversations = Objects.requireNonNull(conversations);
        this.receipts = Objects.requireNonNull(receipts); this.acceptance = Objects.requireNonNull(acceptance);
        this.identities = Objects.requireNonNull(identities); this.clock = Objects.requireNonNull(clock); this.policy = Objects.requireNonNull(policy);
    }
    /** Initial schema only; schema provisioning and migration belong to deployment. */
    public void initialize() {
        jdbc.sql("CREATE TABLE conversation (id CHAR(36) PRIMARY KEY, revision BIGINT NOT NULL, durable_sequence BIGINT NOT NULL, active_task CHAR(36), lease_observed_at TIMESTAMP)").update();
        jdbc.sql("CREATE TABLE task_run (id CHAR(36) PRIMARY KEY, row_version BIGINT NOT NULL, conversation_id CHAR(36) NOT NULL REFERENCES conversation(id), state VARCHAR(32) NOT NULL CHECK (state IN ('QUEUED','RUNNING','SUSPENDED','CANCEL_REQUESTED','COMPLETED','CANCELLED','FAILED')), UNIQUE(id,conversation_id))").update();
        jdbc.sql("ALTER TABLE conversation ADD CONSTRAINT active_task_fk FOREIGN KEY (active_task,id) REFERENCES task_run(id,conversation_id)").update();
        jdbc.sql("CREATE TABLE operation_identity (id CHAR(36) PRIMARY KEY, principal VARCHAR(255) NOT NULL, operation_id CHAR(36) NOT NULL, operation VARCHAR(255) NOT NULL, created_at TIMESTAMP NOT NULL, payload_digest VARCHAR(255) NOT NULL, UNIQUE(principal,operation_id))").update();
        jdbc.sql("CREATE TABLE request_receipt (id CHAR(36) PRIMARY KEY REFERENCES operation_identity(id), row_version BIGINT NOT NULL, conversation_id CHAR(36) NOT NULL REFERENCES conversation(id), expected_revision BIGINT NOT NULL, proof_lease CHAR(36) NOT NULL, proof_revision BIGINT NOT NULL, proof_client CHAR(36) NOT NULL, task_id CHAR(36) NOT NULL REFERENCES task_run(id), revision BIGINT NOT NULL, sequence BIGINT NOT NULL)").update();
        jdbc.sql("CREATE TABLE journal_event (id CHAR(36) PRIMARY KEY, row_version BIGINT NOT NULL, conversation_id CHAR(36) NOT NULL REFERENCES conversation(id), sequence BIGINT NOT NULL, task_id CHAR(36) NOT NULL REFERENCES task_run(id), UNIQUE(conversation_id,sequence))").update();
    }
    public void createConversation(UUID id) { conversations.save(new ConversationRow(id.toString(), null, 0, null)); }
    public Receipt acceptTask(Request request) {
        Objects.requireNonNull(request);
        if (TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("acceptTask must own its transaction");
        Instant now = clock.instant();
        if (request.createdAt().isBefore(now.minus(policy.maxAge())) || request.createdAt().isAfter(now.plus(policy.futureTolerance())))
            throw new IllegalArgumentException("request outside time window");
        try {
            var result = acceptance.accept(request);
            if (result.error() != null) throw new InputLeaseStore.Conflict(result.error());
            return result.receipt();
        }
        catch (DataIntegrityViolationException failure) {
            var prior = receipts.findReceipt(request.principal(), request.operationId().toString());
            if (prior.isEmpty()) {
                if (identities.find(request.principal(), request.operationId()).isPresent()) throw new Conflict("request identity conflict");
                throw failure;
            }
            return sameReceipt(request, prior.get());
        }
    }
    static Receipt sameReceipt(Request request, ReceiptRow stored) {
        if (!stored.operation().equals(request.operation()) || !stored.createdAt().toInstant(ZoneOffset.UTC).equals(request.createdAt())
                || !stored.payloadDigest().equals(request.payloadDigest()) || !stored.conversationId().equals(request.conversationId().toString())
                || stored.expectedRevision() != request.expectedRevision() || !stored.proofLease().equals(request.lease().leaseId().toString())
                || stored.proofRevision() != request.lease().revision() || !stored.proofClient().equals(request.lease().clientInstanceId().toString())) throw new Conflict("request identity conflict");
        if (stored.taskId() == null || stored.revision() == null || stored.sequence() == null) throw new IllegalStateException("incomplete receipt");
        return new Receipt(UUID.fromString(stored.taskId()), UUID.fromString(stored.conversationId()), stored.revision(), stored.sequence());
    }
    static LocalDateTime utcFields(Instant instant) { return LocalDateTime.ofInstant(instant, ZoneOffset.UTC); }
    public Snapshot snapshot(UUID conversationId) {
        return jdbc.sql("SELECT c.id,c.revision,c.durable_sequence,c.active_task,t.state FROM conversation c LEFT JOIN task_run t ON t.id=c.active_task WHERE c.id=:id")
                .param("id", conversationId.toString()).query((rs, row) -> {
                    String active = rs.getString(4);
                    return new Snapshot(UUID.fromString(rs.getString(1)), rs.getLong(2), rs.getLong(3), active == null ? null : UUID.fromString(active), rs.getString(5));
                }).single();
    }
}
