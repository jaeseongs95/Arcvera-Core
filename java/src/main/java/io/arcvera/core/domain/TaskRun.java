package io.arcvera.core.domain;

import java.util.Collection;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.UUID;

/** In-memory task rules. Storage must independently enforce atomicity and CAS. */
public final class TaskRun {
    public enum State { QUEUED, RUNNING, SUSPENDED, CANCEL_REQUESTED, COMPLETED, CANCELLED, FAILED }
    private final UUID id;
    private final Map<UUID, ExecutionTurn> turns = new LinkedHashMap<>();
    private final Map<UUID, SessionEffect> effects = new LinkedHashMap<>();
    private final Map<UUID, UUID> inputEffects = new LinkedHashMap<>();
    private List<UUID> batch = List.of();
    private State state = State.QUEUED;
    public TaskRun(UUID id) { this.id = Objects.requireNonNull(id); }
    public synchronized State state() { return state; }
    public synchronized void start() { require(State.QUEUED); state = State.RUNNING; }
    public synchronized void suspend() { require(State.RUNNING); state = State.SUSPENDED; }
    /** Call only after owner revision reconciliation, not merely reconnecting. */
    public synchronized void resumeReconciled() { require(State.SUSPENDED); state = State.RUNNING; }
    public synchronized void beginBatch(Collection<ExecutionTurn> commands) {
        require(State.RUNNING);
        if (!mayContinueModel()) throw new IllegalStateException("previous batch or effect unresolved");
        var copy = List.copyOf(commands);
        if (copy.isEmpty()) throw new IllegalArgumentException("empty batch");
        var ids = copy.stream().map(ExecutionTurn::id).toList();
        if (ids.stream().distinct().count() != ids.size() || ids.stream().anyMatch(turns::containsKey))
            throw new IllegalArgumentException("duplicate turn id");
        var claimed = new ArrayList<ExecutionTurn>();
        try {
            for (var turn : copy) { turn.claim(this); claimed.add(turn); }
        } catch (RuntimeException failure) {
            claimed.forEach(turn -> turn.releaseUncommittedClaim(this));
            throw failure;
        }
        copy.forEach(turn -> turns.put(turn.id(), turn)); batch = ids;
    }
    public synchronized void recordResponse(UUID turnId, ExecutionTurn.Response response) {
        ExecutionTurn turn = turn(turnId);
        boolean already = turn.response() != null;
        turn.confirm(response);
        if (!already && inputEffects.containsKey(turnId)) effects.get(inputEffects.get(turnId)).inputConfirmed(turnId);
    }
    public synchronized void markUnresolved(UUID turnId) { turn(turnId).markUnresolved(); }
    public synchronized boolean mayContinueModel() { return state == State.RUNNING && batchComplete() && effects.values().stream().noneMatch(SessionEffect::connectionUnconfirmed); }
    public synchronized void addEffect(SessionEffect effect) {
        Objects.requireNonNull(effect);
        if (state != State.RUNNING && state != State.SUSPENDED && state != State.CANCEL_REQUESTED)
            throw new IllegalStateException("task cannot acquire an effect");
        if (!id.equals(effect.taskRunId()) || !turns.containsKey(effect.originTurnId()) || effects.putIfAbsent(effect.id(), effect) != null)
            throw new IllegalArgumentException("unknown origin or duplicate effect");
        if (state == State.CANCEL_REQUESTED) effect.requestStop();
    }
    public synchronized void beginInput(SessionEffect.Authority authority, ExecutionTurn input, long sequence) {
        require(State.RUNNING);
        if (!mayContinueModel() || input.kind() == ExecutionTurn.Kind.MODEL || turns.containsKey(input.id()))
            throw new IllegalStateException("input blocked");
        SessionEffect effect = effects.get(authority.effectId());
        if (effect == null || !id.equals(authority.taskRunId())) throw new IllegalArgumentException("unknown effect");
        input.claim(this);
        try { effect.beginInput(authority, input.id(), sequence); }
        catch (RuntimeException failure) { input.releaseUncommittedClaim(this); throw failure; }
        turns.put(input.id(), input); inputEffects.put(input.id(), effect.id()); batch = List.of(input.id());
    }
    public synchronized void requestCancel() {
        if (state == State.QUEUED) { state = State.CANCELLED; return; }
        if (state == State.COMPLETED || state == State.CANCELLED || state == State.FAILED || state == State.CANCEL_REQUESTED) return;
        state = State.CANCEL_REQUESTED; effects.values().forEach(SessionEffect::requestStop);
    }
    public synchronized void complete(UUID finalModelTurnId) {
        require(State.RUNNING);
        var finalTurn = turn(finalModelTurnId);
        if (finalTurn.kind() != ExecutionTurn.Kind.MODEL || finalTurn.response() == null || !finalTurn.response().finalModelResponse() || !batch.contains(finalModelTurnId))
            throw new IllegalStateException("confirmed final model response required");
        requireSettled(); state = State.COMPLETED;
    }
    public synchronized void confirmCancelled() { require(State.CANCEL_REQUESTED); requireSettled(); state = State.CANCELLED; }
    public synchronized void failConfirmed() {
        if (state != State.RUNNING && state != State.SUSPENDED) throw new IllegalStateException("invalid failure state");
        requireSettled(); state = State.FAILED;
    }
    private void requireSettled() {
        if (turns.values().stream().anyMatch(t -> t.response() == null || t.unresolved())
                || effects.values().stream().anyMatch(e -> !e.closedAndKnown())) throw new IllegalStateException("unsettled work");
    }
    private boolean batchComplete() { return batch.stream().allMatch(id -> turn(id).response() != null && !turn(id).unresolved()); }
    private ExecutionTurn turn(UUID id) { var value = turns.get(id); if (value == null) throw new IllegalArgumentException("unknown turn"); return value; }
    private void require(State expected) { if (state != expected) throw new IllegalStateException("unexpected task state: " + state); }
}
