package io.arcvera.core.store;

import io.arcvera.core.lease.InputLeaseStore;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

/** Invoked through Spring's transaction proxy; duplicate recovery runs after rollback. */
@Service
public class AcceptanceTransaction {
    public record Result(TaskAcceptanceStore.Receipt receipt, String error) { }
    private final JdbcClient jdbc;
    private final TaskRepository tasks;
    private final ReceiptRepository receipts;
    private final EventRepository events;
    private final OperationIdentityService identities;
    private final InputLeaseStore leases;
    AcceptanceTransaction(JdbcClient jdbc, TaskRepository tasks, ReceiptRepository receipts, EventRepository events, OperationIdentityService identities, InputLeaseStore leases) {
        this.jdbc = jdbc; this.tasks = tasks; this.receipts = receipts; this.events = events; this.identities = identities; this.leases = leases;
    }
    @Transactional
    public Result accept(TaskAcceptanceStore.Request request) {
        var prior = receipts.findReceipt(request.principal(), request.operationId().toString());
        if (prior.isPresent()) return new Result(TaskAcceptanceStore.sameReceipt(request, prior.get()), null);
        if (identities.find(request.principal(), request.operationId()).isPresent()) throw new TaskAcceptanceStore.Conflict("request identity conflict");
        UUID identity = identities.reserve(request.principal(), request.operationId(), request.operation(), request.createdAt(), request.payloadDigest());
        try { leases.requireProof(request.principal(), request.conversationId(), request.lease()); }
        catch (InputLeaseStore.Conflict rejected) {
            identities.discard(identity);
            return new Result(null, rejected.getMessage());
        }
        String conversation = request.conversationId().toString();
        String task = UUID.randomUUID().toString();
        tasks.save(new TaskRow(task, null, conversation, "QUEUED"));
        int changed = jdbc.sql("UPDATE conversation SET revision=revision+1,durable_sequence=durable_sequence+1,active_task=:task WHERE id=:conversation AND revision=:revision AND active_task IS NULL")
                .param("task", task).param("conversation", conversation).param("revision", request.expectedRevision()).update();
        if (changed != 1) throw new TaskAcceptanceStore.Conflict("stale or active conversation");
        long sequence = jdbc.sql("SELECT durable_sequence FROM conversation WHERE id=:conversation").param("conversation", conversation).query(Long.class).single();
        long revision = request.expectedRevision() + 1;
        events.save(new EventRow(UUID.randomUUID().toString(), null, conversation, sequence, task));
        jdbc.sql("INSERT INTO request_receipt(id,row_version,conversation_id,expected_revision,proof_lease,proof_revision,proof_client,task_id,revision,sequence) VALUES (:id,0,:conversation,:expected,:lease,:leaseRevision,:client,:task,:revision,:sequence)")
                .param("id", identity.toString()).param("conversation", conversation).param("expected", request.expectedRevision())
                .param("lease", request.lease().leaseId().toString()).param("leaseRevision", request.lease().revision()).param("client", request.lease().clientInstanceId().toString())
                .param("task", task).param("revision", revision).param("sequence", sequence).update();
        return new Result(new TaskAcceptanceStore.Receipt(UUID.fromString(task), request.conversationId(), revision, sequence), null);
    }
}
