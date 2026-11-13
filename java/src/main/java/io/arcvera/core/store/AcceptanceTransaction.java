package io.arcvera.core.store;

import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

/** Invoked through Spring's transaction proxy; duplicate recovery runs after rollback. */
@Service
public class AcceptanceTransaction {
    private final JdbcClient jdbc;
    private final TaskRepository tasks;
    private final ReceiptRepository receipts;
    private final EventRepository events;
    AcceptanceTransaction(JdbcClient jdbc, TaskRepository tasks, ReceiptRepository receipts, EventRepository events) {
        this.jdbc = jdbc; this.tasks = tasks; this.receipts = receipts; this.events = events;
    }
    @Transactional
    public TaskAcceptanceStore.Receipt accept(TaskAcceptanceStore.Request request) {
        var prior = receipts.findReceipt(request.principal(), request.operationId().toString());
        if (prior.isPresent()) return TaskAcceptanceStore.sameReceipt(request, prior.get());
        String conversation = request.conversationId().toString();
        var reservation = receipts.save(new ReceiptRow(UUID.randomUUID().toString(), null, request.principal(), request.operationId().toString(),
                request.operation(), TaskAcceptanceStore.utcFields(request.createdAt()), request.payloadDigest(), conversation,
                request.expectedRevision(), null, null, null));
        String task = UUID.randomUUID().toString();
        tasks.save(new TaskRow(task, null, conversation, "QUEUED"));
        int changed = jdbc.sql("UPDATE conversation SET revision=revision+1,durable_sequence=durable_sequence+1,active_task=:task WHERE id=:conversation AND revision=:revision AND active_task IS NULL")
                .param("task", task).param("conversation", conversation).param("revision", request.expectedRevision()).update();
        if (changed != 1) throw new TaskAcceptanceStore.Conflict("stale or active conversation");
        long sequence = jdbc.sql("SELECT durable_sequence FROM conversation WHERE id=:conversation").param("conversation", conversation).query(Long.class).single();
        long revision = request.expectedRevision() + 1;
        events.save(new EventRow(UUID.randomUUID().toString(), null, conversation, sequence, task));
        receipts.save(new ReceiptRow(reservation.id(), reservation.rowVersion(), reservation.principal(), reservation.operationId(), reservation.operation(),
                reservation.createdAt(), reservation.payloadDigest(), reservation.conversationId(), reservation.expectedRevision(), task, revision, sequence));
        return new TaskAcceptanceStore.Receipt(UUID.fromString(task), request.conversationId(), revision, sequence);
    }
}
