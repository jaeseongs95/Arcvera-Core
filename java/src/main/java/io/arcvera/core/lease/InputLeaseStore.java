package io.arcvera.core.lease;

import io.arcvera.core.store.OperationIdentityService;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.time.temporal.ChronoUnit;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.dao.DataIntegrityViolationException;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Service;
import org.springframework.transaction.PlatformTransactionManager;
import org.springframework.transaction.support.TransactionSynchronizationManager;
import org.springframework.transaction.support.TransactionTemplate;

/** Caller supplies a principal already authenticated and authorized for the conversation. */
@Service
public final class InputLeaseStore {
    public enum Mode { GUI, TERMINAL }
    public enum State { HELD, RELEASED, EXPIRED }
    public enum Action { ACQUIRE, RENEW, RELEASE }
    public record Policy(Duration ttl, Duration maxAge, Duration futureTolerance, Duration grace, Duration retention) {
        public Policy {
            Objects.requireNonNull(ttl); Objects.requireNonNull(maxAge); Objects.requireNonNull(futureTolerance);
            Objects.requireNonNull(grace); Objects.requireNonNull(retention);
            if (ttl.isNegative() || ttl.isZero() || ttl.getNano() % 1_000_000 != 0 || maxAge.isNegative() || maxAge.isZero()
                    || futureTolerance.isNegative() || grace.isNegative() || retention.compareTo(maxAge.plus(grace)) < 0)
                throw new IllegalArgumentException("invalid lease policy");
        }
    }
    public record Proof(UUID leaseId, long revision, UUID clientInstanceId) {
        public Proof {
            Objects.requireNonNull(leaseId); Objects.requireNonNull(clientInstanceId);
            if (revision < 0) throw new IllegalArgumentException("negative revision");
        }
    }
    public record Request(String principal, UUID operationId, Instant createdAt, String digest,
                          UUID conversationId, long expectedRevision, UUID clientInstanceId,
                          Action action, UUID leaseId, Mode mode) {
        public Request {
            Objects.requireNonNull(principal); Objects.requireNonNull(operationId); Objects.requireNonNull(createdAt);
            Objects.requireNonNull(digest); Objects.requireNonNull(conversationId); Objects.requireNonNull(clientInstanceId);
            Objects.requireNonNull(action);
            if (principal.isBlank() || digest.isBlank() || expectedRevision < 0 || createdAt.getNano() % 1_000_000 != 0
                    || (action == Action.ACQUIRE && (leaseId != null || mode == null))
                    || (action != Action.ACQUIRE && (leaseId == null || mode != null))) throw new IllegalArgumentException("invalid request");
        }
    }
    public record Lease(UUID conversationId, UUID leaseId, long revision, UUID clientInstanceId,
                        Mode mode, State state, Instant expiresAt, Instant serverTime) { }
    public static final class Conflict extends RuntimeException {
        private static final long serialVersionUID = 1L;
        public Conflict(String message) { super(message); }
    }
    private record Stored(Lease lease, String principal) { }
    private record Outcome(Lease lease, String error) { }
    private final JdbcClient jdbc;
    private final OperationIdentityService identities;
    private final TransactionTemplate transaction;
    private final Clock clock;
    private final Policy policy;
    public InputLeaseStore(JdbcClient jdbc, OperationIdentityService identities, PlatformTransactionManager manager, Clock clock, Policy policy) {
        this.jdbc = jdbc; this.identities = identities; this.transaction = new TransactionTemplate(manager); this.clock = clock; this.policy = policy;
    }
    public void initialize() {
        jdbc.sql("CREATE TABLE input_lease (conversation_id CHAR(36) PRIMARY KEY REFERENCES conversation(id), lease_id CHAR(36) NOT NULL UNIQUE, revision BIGINT NOT NULL, principal VARCHAR(255) NOT NULL, client_id CHAR(36) NOT NULL, input_mode VARCHAR(16) NOT NULL CHECK (input_mode IN ('GUI','TERMINAL')), state VARCHAR(16) NOT NULL CHECK (state IN ('HELD','RELEASED')), expires_at TIMESTAMP NOT NULL)").update();
        jdbc.sql("CREATE TABLE lease_receipt (id CHAR(36) PRIMARY KEY REFERENCES operation_identity(id), conversation_id CHAR(36) NOT NULL REFERENCES conversation(id), expected_revision BIGINT NOT NULL, requested_client CHAR(36) NOT NULL, requested_lease CHAR(36), requested_mode VARCHAR(16), lease_id CHAR(36) NOT NULL, revision BIGINT NOT NULL, client_id CHAR(36) NOT NULL, input_mode VARCHAR(16) NOT NULL, state VARCHAR(16) NOT NULL, expires_at TIMESTAMP NOT NULL, server_time TIMESTAMP NOT NULL)").update();
    }
    private Instant now() { return clock.instant().truncatedTo(ChronoUnit.MILLIS); }
    private static LocalDateTime fields(Instant instant) { return LocalDateTime.ofInstant(instant, ZoneOffset.UTC); }
    private static Instant instant(java.sql.ResultSet rs, String column) throws java.sql.SQLException {
        return rs.getObject(column, LocalDateTime.class).toInstant(ZoneOffset.UTC);
    }
    private Optional<Stored> stored(UUID conversation) {
        return jdbc.sql("SELECT * FROM input_lease WHERE conversation_id=:conversation").param("conversation", conversation.toString())
                .query((rs, n) -> new Stored(new Lease(conversation, UUID.fromString(rs.getString("lease_id")), rs.getLong("revision"),
                        UUID.fromString(rs.getString("client_id")), Mode.valueOf(rs.getString("input_mode")),
                        State.valueOf(rs.getString("state")), instant(rs, "expires_at"), now()), rs.getString("principal"))).optional();
    }
    public Optional<Lease> query(UUID conversation) {
        if (TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("query must own its transaction");
        return Objects.requireNonNull(transaction.execute(status -> {
            Instant time = observe(conversation);
            return stored(conversation).map(s -> project(s.lease(), time));
        }));
    }
    private static Lease project(Lease lease, Instant time) {
        State state = lease.state() == State.HELD && !time.isBefore(lease.expiresAt()) ? State.EXPIRED : lease.state();
        return new Lease(lease.conversationId(), lease.leaseId(), lease.revision(), lease.clientInstanceId(), lease.mode(), state, lease.expiresAt(), time);
    }
    public Lease change(Request request) {
        if (TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("change must own its transaction");
        Instant time = now();
        if (request.createdAt().isBefore(time.minus(policy.maxAge())) || request.createdAt().isAfter(time.plus(policy.futureTolerance())))
            throw new IllegalArgumentException("request outside time window");
        try {
            Outcome outcome = Objects.requireNonNull(transaction.execute(status -> apply(request)));
            if (outcome.error() != null) throw new Conflict(outcome.error());
            return outcome.lease();
        } catch (DataIntegrityViolationException failure) {
            var existing = identities.find(request.principal(), request.operationId());
            if (existing.isEmpty()) throw failure;
            return replay(request, existing.get());
        }
    }
    private Outcome apply(Request request) {
        var existing = identities.find(request.principal(), request.operationId());
        if (existing.isPresent()) return new Outcome(replay(request, existing.get()), null);
        UUID identity = identities.reserve(request.principal(), request.operationId(), request.action().name(), request.createdAt(), request.digest());
        Instant time;
        Optional<Stored> current;
        long oldRevision;
        try {
            time = observe(request.conversationId());
            current = stored(request.conversationId());
            oldRevision = current.map(s -> s.lease().revision()).orElse(0L);
            if (oldRevision != request.expectedRevision() || oldRevision == Long.MAX_VALUE) throw new Conflict("stale revision");
            if (request.action() == Action.ACQUIRE) {
                if (current.isPresent() && project(current.get().lease(), time).state() == State.HELD) throw new Conflict("lease held");
            } else {
                if (current.isEmpty()) throw new Conflict("lease absent");
                Stored s = current.get();
                if (!s.principal().equals(request.principal()) || !s.lease().clientInstanceId().equals(request.clientInstanceId())
                        || !s.lease().leaseId().equals(request.leaseId()) || project(s.lease(), time).state() != State.HELD)
                    throw new Conflict("lease proof rejected");
            }
        } catch (Conflict rejected) {
            identities.discard(identity);
            return new Outcome(null, rejected.getMessage());
        }
        UUID leaseId = request.action() == Action.ACQUIRE ? UUID.randomUUID() : request.leaseId();
        Mode mode = request.action() == Action.ACQUIRE ? request.mode() : current.orElseThrow().lease().mode();
        State state = request.action() == Action.RELEASE ? State.RELEASED : State.HELD;
        Instant expires = request.action() == Action.RELEASE ? current.orElseThrow().lease().expiresAt() : time.plus(policy.ttl());
        Lease result = new Lease(request.conversationId(), leaseId, oldRevision + 1, request.clientInstanceId(), mode, state, expires, time);
        if (current.isEmpty()) {
            jdbc.sql("INSERT INTO input_lease(conversation_id,lease_id,revision,principal,client_id,input_mode,state,expires_at) VALUES (:conversation,:lease,:revision,:principal,:client,:mode,:state,:expires)")
                    .param("conversation", request.conversationId().toString()).param("lease", leaseId.toString()).param("revision", result.revision())
                    .param("principal", request.principal()).param("client", request.clientInstanceId().toString()).param("mode", mode.name())
                    .param("state", state.name()).param("expires", fields(expires)).update();
        } else {
            int changed = jdbc.sql("UPDATE input_lease SET lease_id=:lease,revision=:next,principal=:principal,client_id=:client,input_mode=:mode,state=:state,expires_at=:expires WHERE conversation_id=:conversation AND revision=:expected")
                    .param("lease", leaseId.toString()).param("next", result.revision()).param("principal", request.principal())
                    .param("client", request.clientInstanceId().toString()).param("mode", mode.name()).param("state", state.name())
                    .param("expires", fields(expires)).param("conversation", request.conversationId().toString()).param("expected", oldRevision).update();
            if (changed != 1) throw new Conflict("stale revision");
        }
        jdbc.sql("INSERT INTO lease_receipt(id,conversation_id,expected_revision,requested_client,requested_lease,requested_mode,lease_id,revision,client_id,input_mode,state,expires_at,server_time) VALUES (:id,:conversation,:expected,:requestedClient,:requestedLease,:requestedMode,:lease,:revision,:client,:mode,:state,:expires,:time)")
                .param("id", identity.toString()).param("conversation", request.conversationId().toString()).param("expected", request.expectedRevision())
                .param("requestedClient", request.clientInstanceId().toString()).param("requestedLease", request.leaseId() == null ? null : request.leaseId().toString())
                .param("requestedMode", request.mode() == null ? null : request.mode().name()).param("lease", leaseId.toString()).param("revision", result.revision())
                .param("client", result.clientInstanceId().toString()).param("mode", mode.name()).param("state", state.name())
                .param("expires", fields(expires)).param("time", fields(time)).update();
        return new Outcome(result, null);
    }
    public void lockConversation(UUID conversation) {
        if (!TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("transaction required");
        if (jdbc.sql("UPDATE conversation SET revision=revision WHERE id=:id").param("id", conversation.toString()).update() != 1)
            throw new Conflict("conversation absent");
    }
    private Instant observe(UUID conversation) {
        lockConversation(conversation);
        Instant time = now();
        LocalDateTime previous = jdbc.sql("SELECT COALESCE(lease_observed_at,:time) FROM conversation WHERE id=:id")
                .param("time", fields(time)).param("id", conversation.toString())
                .query((rs, row) -> rs.getObject(1, LocalDateTime.class)).single();
        if (time.isBefore(previous.toInstant(ZoneOffset.UTC))) throw new Conflict("clock moved backwards");
        jdbc.sql("UPDATE conversation SET lease_observed_at=:time WHERE id=:id").param("time", fields(time)).param("id", conversation.toString()).update();
        return time;
    }
    /** Must run after lockConversation within the same transaction that records the input. */
    public void requireProof(String principal, UUID conversation, Proof proof) {
        if (!TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("transaction required");
        Instant time = observe(conversation);
        Stored s = stored(conversation).orElseThrow(() -> new Conflict("lease absent"));
        if (!s.principal().equals(principal) || !s.lease().leaseId().equals(proof.leaseId())
                || s.lease().revision() != proof.revision() || !s.lease().clientInstanceId().equals(proof.clientInstanceId())
                || project(s.lease(), time).state() != State.HELD) throw new Conflict("lease proof rejected");
    }
    private Lease replay(Request request, OperationIdentityService.Identity identity) {
        if (!identity.operation().equals(request.action().name()) || !identity.createdAt().equals(request.createdAt()) || !identity.digest().equals(request.digest()))
            throw new Conflict("request identity conflict");
        return jdbc.sql("SELECT * FROM lease_receipt WHERE id=:id").param("id", identity.id().toString()).query((rs, n) -> {
            String requestedLease = rs.getString("requested_lease"); String requestedMode = rs.getString("requested_mode");
            if (!rs.getString("conversation_id").equals(request.conversationId().toString()) || rs.getLong("expected_revision") != request.expectedRevision()
                    || !rs.getString("requested_client").equals(request.clientInstanceId().toString())
                    || !Objects.equals(requestedLease, request.leaseId() == null ? null : request.leaseId().toString())
                    || !Objects.equals(requestedMode, request.mode() == null ? null : request.mode().name())) throw new Conflict("request identity conflict");
            return new Lease(request.conversationId(), UUID.fromString(rs.getString("lease_id")), rs.getLong("revision"),
                    UUID.fromString(rs.getString("client_id")), Mode.valueOf(rs.getString("input_mode")), State.valueOf(rs.getString("state")),
                    instant(rs, "expires_at"), instant(rs, "server_time"));
        }).optional().orElseThrow(() -> new Conflict("request identity conflict"));
    }
}
