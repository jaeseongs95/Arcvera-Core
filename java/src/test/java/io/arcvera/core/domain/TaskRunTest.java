package io.arcvera.core.domain;

import static org.junit.jupiter.api.Assertions.*;

import java.util.List;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;

class TaskRunTest {
    private final UUID taskId = UUID.randomUUID();
    private final TaskRun task = new TaskRun(taskId);
    private final ExecutionTurn origin = turn(ExecutionTurn.Kind.TOOL);
    private final SessionEffect effect = new SessionEffect(UUID.randomUUID(), origin.id(), taskId, "owner", 3, 7);
    private ExecutionTurn turn(ExecutionTurn.Kind kind) { return new ExecutionTurn(UUID.randomUUID(), "digest", kind); }
    private SessionEffect.Authority authority() { return new SessionEffect.Authority("owner", taskId, effect.id(), 3, 7, true); }
    private void prepare() { task.start(); task.beginBatch(List.of(origin)); task.addEffect(effect); task.recordResponse(origin.id(), new ExecutionTurn.Response("session")); }
    @Test void liveSessionAllowsNextModelButNotCompletion() {
        prepare(); assertTrue(task.mayContinueModel()); assertThrows(IllegalStateException.class, () -> task.complete(origin.id()));
        var input = turn(ExecutionTurn.Kind.TOOL); task.beginInput(authority(), input, 1);
        assertFalse(task.mayContinueModel()); task.recordResponse(input.id(), new ExecutionTurn.Response("done"));
        effect.confirmClosed(); var finalTurn = turn(ExecutionTurn.Kind.MODEL); task.beginBatch(List.of(finalTurn)); task.recordResponse(finalTurn.id(), new ExecutionTurn.Response("final", true)); task.complete(finalTurn.id()); assertEquals(TaskRun.State.COMPLETED, task.state());
    }
    @Test void batchWaitsForEachResponseAndRejectsConflictingDuplicates() {
        task.start(); var second = turn(ExecutionTurn.Kind.TOOL); task.beginBatch(List.of(origin, second));
        task.recordResponse(second.id(), new ExecutionTurn.Response("second"));
        task.recordResponse(second.id(), new ExecutionTurn.Response("second")); assertFalse(task.mayContinueModel());
        assertThrows(IllegalStateException.class, () -> task.recordResponse(second.id(), new ExecutionTurn.Response("changed")));
        task.recordResponse(origin.id(), new ExecutionTurn.Response("first")); assertTrue(task.mayContinueModel());
    }
    @Test void rejectsWrongAuthorityAndUnresolvedInput() {
        prepare();
        for (var a : List.of(new SessionEffect.Authority("other", taskId, effect.id(), 3, 7, true),
                new SessionEffect.Authority("owner", UUID.randomUUID(), effect.id(), 3, 7, true),
                new SessionEffect.Authority("owner", taskId, effect.id(), 4, 7, true),
                new SessionEffect.Authority("owner", taskId, effect.id(), 3, 8, true),
                new SessionEffect.Authority("owner", taskId, effect.id(), 3, 7, false))) {
            assertThrows(IllegalArgumentException.class, () -> task.beginInput(a, turn(ExecutionTurn.Kind.TOOL), 1));
        }
        var first = turn(ExecutionTurn.Kind.TOOL); task.beginInput(authority(), first, 1); task.markUnresolved(first.id());
        assertThrows(IllegalStateException.class, () -> task.beginInput(authority(), turn(ExecutionTurn.Kind.TOOL), 2));
        task.recordResponse(first.id(), new ExecutionTurn.Response("confirmed"));
        assertThrows(IllegalStateException.class, () -> task.beginInput(authority(), turn(ExecutionTurn.Kind.TOOL), 1));
        task.beginInput(authority(), turn(ExecutionTurn.Kind.TOOL), 2);
    }
    @Test void rejectsEffectFromAnotherTask() {
        task.start(); task.beginBatch(List.of(origin));
        var foreign = new SessionEffect(UUID.randomUUID(), origin.id(), UUID.randomUUID(), "owner", 3, 7);
        assertThrows(IllegalArgumentException.class, () -> task.addEffect(foreign));
    }
    @Test void concurrentInputsHaveOneWinner() throws Exception {
        prepare(); var start = new CountDownLatch(1);
        try (var pool = Executors.newFixedThreadPool(2)) {
            var attempts = List.of(pool.submit(() -> attempt(start)), pool.submit(() -> attempt(start)));
            start.countDown(); int successes = 0;
            for (var f : attempts) if (f.get(5, TimeUnit.SECONDS)) successes++;
            assertEquals(1, successes);
        }
    }
    private boolean attempt(CountDownLatch latch) throws InterruptedException {
        latch.await(); try { task.beginInput(authority(), turn(ExecutionTurn.Kind.TOOL), 1); return true; }
        catch (IllegalStateException expected) { return false; }
    }
    @Test void cancellationAndCompletionOrdering() {
        prepare(); task.requestCancel(); assertEquals(SessionEffect.State.STOP_REQUESTED, effect.state());
        assertThrows(IllegalStateException.class, () -> task.beginInput(authority(), turn(ExecutionTurn.Kind.CONTROL), 1));
        assertThrows(IllegalStateException.class, () -> task.complete(origin.id())); assertThrows(IllegalStateException.class, task::confirmCancelled);
        effect.confirmClosed(); task.confirmCancelled(); assertEquals(TaskRun.State.CANCELLED, task.state());
        var other = new TaskRun(UUID.randomUUID()); other.start(); var finalTurn = turn(ExecutionTurn.Kind.MODEL); other.beginBatch(List.of(finalTurn)); other.recordResponse(finalTurn.id(), new ExecutionTurn.Response("final", true)); other.complete(finalTurn.id()); other.requestCancel();
        assertEquals(TaskRun.State.COMPLETED, other.state());
    }
    @Test void closedEffectStaysClosedAfterDuplicateResponseAndConnectionLossProtects() {
        prepare(); effect.connectionLost(); assertFalse(task.mayContinueModel()); assertThrows(IllegalStateException.class, () -> task.beginInput(authority(), turn(ExecutionTurn.Kind.CONTROL), 1)); assertThrows(IllegalStateException.class, () -> task.complete(origin.id()));
        effect.connectionReconciled(); effect.confirmClosed(); task.recordResponse(origin.id(), new ExecutionTurn.Response("session"));
        assertEquals(SessionEffect.State.CLOSED, effect.state()); var finalTurn = turn(ExecutionTurn.Kind.MODEL); task.beginBatch(List.of(finalTurn)); task.recordResponse(finalTurn.id(), new ExecutionTurn.Response("final", true)); task.complete(finalTurn.id());
    }
    @Test void nonFinalModelResponseCannotCompleteTask() {
        task.start(); var model = turn(ExecutionTurn.Kind.MODEL); task.beginBatch(List.of(model));
        task.recordResponse(model.id(), new ExecutionTurn.Response("tool proposal"));
        assertThrows(IllegalStateException.class, () -> task.complete(model.id()));
        assertEquals(TaskRun.State.RUNNING, task.state());
    }
    @Test void finalModelResponseCannotCompleteWhileEffectSurvives() {
        prepare(); var model = turn(ExecutionTurn.Kind.MODEL); task.beginBatch(List.of(model));
        task.recordResponse(model.id(), new ExecutionTurn.Response("final", true));
        assertThrows(IllegalStateException.class, () -> task.complete(model.id()));
        assertEquals(TaskRun.State.RUNNING, task.state());
        effect.confirmClosed(); task.complete(model.id());
        assertEquals(TaskRun.State.COMPLETED, task.state());
    }
    @Test void terminalTaskCannotAcquireLateEffect() {
        task.start(); task.beginBatch(List.of(origin)); task.recordResponse(origin.id(), new ExecutionTurn.Response("done"));
        var model = turn(ExecutionTurn.Kind.MODEL); task.beginBatch(List.of(model));
        task.recordResponse(model.id(), new ExecutionTurn.Response("final", true)); task.complete(model.id());
        assertThrows(IllegalStateException.class, () -> task.addEffect(effect));

        var cancelled = new TaskRun(UUID.randomUUID()); cancelled.start();
        var cancelledOrigin = turn(ExecutionTurn.Kind.TOOL); cancelled.beginBatch(List.of(cancelledOrigin));
        cancelled.recordResponse(cancelledOrigin.id(), new ExecutionTurn.Response("done"));
        cancelled.requestCancel(); cancelled.confirmCancelled();
        var late = new SessionEffect(UUID.randomUUID(), cancelledOrigin.id(), UUID.randomUUID(), "owner", 3, 7);
        assertThrows(IllegalStateException.class, () -> cancelled.addEffect(late));
    }
    @Test void turnCannotBeAdoptedByAnotherTaskBeforeOrAfterResponse() {
        task.start(); task.beginBatch(List.of(origin));
        var other = new TaskRun(UUID.randomUUID()); other.start();
        assertThrows(IllegalStateException.class, () -> other.beginBatch(List.of(origin)));
        task.recordResponse(origin.id(), new ExecutionTurn.Response("done"));
        assertThrows(IllegalStateException.class, () -> other.beginBatch(List.of(origin)));
        var model = turn(ExecutionTurn.Kind.MODEL); task.beginBatch(List.of(model));
        task.recordResponse(model.id(), new ExecutionTurn.Response("final", true));
        assertThrows(IllegalStateException.class, () -> other.beginBatch(List.of(model)));
    }
    @Test void rejectedBatchReleasesOnlyItsUncommittedClaims() {
        task.start(); task.beginBatch(List.of(origin));
        var other = new TaskRun(UUID.randomUUID()); other.start();
        var fresh = turn(ExecutionTurn.Kind.TOOL);
        assertThrows(IllegalStateException.class, () -> other.beginBatch(List.of(fresh, origin)));
        other.beginBatch(List.of(fresh));
        other.recordResponse(fresh.id(), new ExecutionTurn.Response("accepted"));
        assertTrue(other.mayContinueModel());
    }
    @Test void effectReportedAfterCancellationRequestRemainsOwnedAndNeedsClosure() {
        task.start(); task.beginBatch(List.of(origin)); task.requestCancel();
        task.addEffect(effect);
        assertEquals(SessionEffect.State.STOP_REQUESTED, effect.state());
        task.recordResponse(origin.id(), new ExecutionTurn.Response("session"));
        assertThrows(IllegalStateException.class, task::confirmCancelled);
        effect.confirmClosed(); task.confirmCancelled();
    }
}
