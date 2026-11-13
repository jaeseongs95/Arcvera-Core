package io.arcvera.core.domain;

import java.util.Objects;
import java.util.UUID;

/** One immutable command identity and its one immutable, confirmed response. */
public final class ExecutionTurn {
    public enum Kind { MODEL, TOOL, CONTROL }
    public record Response(String value, boolean finalModelResponse) {
        public Response { Objects.requireNonNull(value); }
        public Response(String value) { this(value, false); }
    }
    private final UUID id;
    private final String commandDigest;
    private final Kind kind;
    private Response response;
    private boolean unresolved;
    private TaskRun owner;

    public ExecutionTurn(UUID id, String commandDigest, Kind kind) {
        this.id = Objects.requireNonNull(id);
        this.commandDigest = Objects.requireNonNull(commandDigest);
        this.kind = Objects.requireNonNull(kind);
        if (commandDigest.isBlank()) throw new IllegalArgumentException("empty command digest");
    }
    public UUID id() { return id; }
    public String commandDigest() { return commandDigest; }
    public Kind kind() { return kind; }
    public synchronized Response response() { return response; }
    public synchronized boolean unresolved() { return unresolved; }
    synchronized void claim(TaskRun task) {
        Objects.requireNonNull(task);
        if (owner != null || response != null || unresolved) throw new IllegalStateException("turn already adopted");
        owner = task;
    }
    synchronized void releaseUncommittedClaim(TaskRun task) {
        if (owner != task || response != null || unresolved) throw new IllegalStateException("invalid turn claim rollback");
        owner = null;
    }
    synchronized void markUnresolved() { if (response == null) unresolved = true; }
    synchronized void confirm(Response result) {
        Objects.requireNonNull(result);
        if (response != null && !response.equals(result)) throw new IllegalStateException("conflicting response");
        response = result;
        unresolved = false;
    }
}
