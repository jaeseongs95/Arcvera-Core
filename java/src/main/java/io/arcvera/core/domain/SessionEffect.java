package io.arcvera.core.domain;

import java.util.Objects;
import java.util.UUID;

/** A surviving effect; disconnection is observation, never proof of closure. */
public final class SessionEffect {
    public enum State { ACTIVE, STOP_REQUESTED, CLOSED }
    public record Authority(String principal, UUID taskRunId, UUID effectId, long ownerEpoch, long handleGeneration, boolean approved) {
        public Authority { Objects.requireNonNull(principal); Objects.requireNonNull(taskRunId); Objects.requireNonNull(effectId); }
    }
    private final UUID id;
    private final UUID originTurnId;
    private final UUID taskRunId;
    private final String principal;
    private final long ownerEpoch;
    private final long handleGeneration;
    private State state = State.ACTIVE;
    private boolean connectionUnconfirmed;
    private UUID inputInFlight;
    private long nextSequence = 1;

    public SessionEffect(UUID id, UUID originTurnId, UUID taskRunId, String principal, long ownerEpoch, long handleGeneration) {
        this.id = Objects.requireNonNull(id); this.originTurnId = Objects.requireNonNull(originTurnId);
        this.taskRunId = Objects.requireNonNull(taskRunId); this.principal = Objects.requireNonNull(principal);
        this.ownerEpoch = ownerEpoch; this.handleGeneration = handleGeneration;
    }
    public UUID id() { return id; }
    public UUID originTurnId() { return originTurnId; }
    UUID taskRunId() { return taskRunId; }
    public synchronized State state() { return state; }
    public synchronized boolean connectionUnconfirmed() { return connectionUnconfirmed; }
    public synchronized void connectionLost() { connectionUnconfirmed = true; }
    /** Caller must first reconcile owner evidence; mere reconnection is insufficient. */
    public synchronized void connectionReconciled() { connectionUnconfirmed = false; }
    synchronized void beginInput(Authority authority, UUID turnId, long sequence) {
        Objects.requireNonNull(authority); Objects.requireNonNull(turnId);
        if (!principal.equals(authority.principal()) || !taskRunId.equals(authority.taskRunId())
                || !id.equals(authority.effectId()) || ownerEpoch != authority.ownerEpoch()
                || handleGeneration != authority.handleGeneration() || !authority.approved())
            throw new IllegalArgumentException("input authority mismatch");
        if (state != State.ACTIVE || connectionUnconfirmed || inputInFlight != null || sequence != nextSequence)
            throw new IllegalStateException("input blocked");
        inputInFlight = turnId;
    }
    synchronized void inputConfirmed(UUID turnId) {
        if (!Objects.equals(inputInFlight, turnId)) throw new IllegalStateException("unexpected input response");
        inputInFlight = null; nextSequence++;
    }
    synchronized void requestStop() { if (state == State.ACTIVE) state = State.STOP_REQUESTED; }
    /** Caller supplies confirmed termination and cleanup, not a socket close or timeout. */
    public synchronized void confirmClosed() { state = State.CLOSED; connectionUnconfirmed = false; }
    synchronized boolean closedAndKnown() { return state == State.CLOSED && !connectionUnconfirmed && inputInFlight == null; }
}
