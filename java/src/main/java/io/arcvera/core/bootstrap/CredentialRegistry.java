package io.arcvera.core.bootstrap;

import java.security.MessageDigest;
import java.io.IOException;
import java.security.NoSuchAlgorithmException;
import java.security.SecureRandom;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Base64;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import java.util.function.LongSupplier;

/** Process-local state; one instance must be owned for the lifetime of one Core instance. */
public final class CredentialRegistry {
    private static final Duration TTL = Duration.ofMinutes(15);
    private static final Duration ROTATE_BEFORE = Duration.ofMinutes(2);
    private static final Duration GRACE = Duration.ofSeconds(30);
    private final Clock clock;
    private final SecureRandom random;
    private final LongSupplier ticker;
    private final UUID instanceId;
    private final String osPrincipal;
    private final Set<String> scopes;
    private Entry current;
    private Entry previous;
    private Instant previousUntil;
    private long rotationTick;
    private Instant lastObserved;
    private boolean revoked;

    public CredentialRegistry(Clock clock, SecureRandom random, LongSupplier ticker, UUID instanceId,
                              String osPrincipal, Set<String> scopes) {
        this.clock = Objects.requireNonNull(clock);
        this.random = Objects.requireNonNull(random);
        this.ticker = Objects.requireNonNull(ticker);
        this.instanceId = Objects.requireNonNull(instanceId);
        this.osPrincipal = Objects.requireNonNull(osPrincipal);
        this.scopes = Set.copyOf(scopes);
        if (osPrincipal.isBlank() || this.scopes.isEmpty() || this.scopes.stream().anyMatch(String::isBlank)) {
            throw new IllegalArgumentException("Principal and scopes must be nonempty");
        }
    }

    // ceiling: one lock per Core instance; revisit only if measured authentication throughput requires it.
    public synchronized IssuedCredential issue() {
        if (current != null || revoked) throw new IllegalStateException("Credential already issued or revoked");
        Candidate candidate = create(0, now(), ticker.getAsLong());
        current = candidate.entry;
        return candidate.credential;
    }

    /** Holds authentication locked: a confirmed non-commit preserves state; an uncertain failure revokes it. */
    public synchronized IssuedCredential issueAndPublish(CredentialPublisher publisher) throws IOException {
        Objects.requireNonNull(publisher);
        if (current != null || revoked) throw new IllegalStateException("Credential already issued or revoked");
        Candidate candidate = create(0, now(), ticker.getAsLong());
        publish(publisher, candidate.credential);
        current = candidate.entry;
        return candidate.credential;
    }

    public synchronized IssuedCredential rotate(UUID expectedCredentialId, long expectedGeneration) {
        Instant now = now();
        long tick = ticker.getAsLong();
        checkRotation(expectedCredentialId, expectedGeneration, now, tick);
        Candidate candidate = create(current.generation + 1, now, tick);
        installRotation(candidate, now, tick);
        return candidate.credential;
    }

    private void checkRotation(UUID expectedCredentialId, long expectedGeneration, Instant now, long tick) {
        if (revoked || current == null || !current.id.equals(expectedCredentialId)
                || current.generation != expectedGeneration
                || (now.isBefore(current.expiresAt.minus(ROTATE_BEFORE)) && !elapsed(tick, current.issuedTick, TTL.minus(ROTATE_BEFORE)))
                || !now.isBefore(current.expiresAt) || elapsed(tick, current.issuedTick, TTL)
                || current.generation == Long.MAX_VALUE) {
            throw new IllegalStateException("Credential rotation rejected");
        }
    }

    public synchronized IssuedCredential rotateAndPublish(UUID expectedCredentialId, long expectedGeneration,
                                                            CredentialPublisher publisher) throws IOException {
        Objects.requireNonNull(publisher);
        Instant now = now();
        long tick = ticker.getAsLong();
        checkRotation(expectedCredentialId, expectedGeneration, now, tick);
        Candidate candidate = create(current.generation + 1, now, tick);
        publish(publisher, candidate.credential);
        installRotation(candidate, now, tick);
        return candidate.credential;
    }

    private void installRotation(Candidate candidate, Instant now, long tick) {
        Entry old = current;
        current = candidate.entry;
        previous = old;
        previousUntil = now.plus(GRACE);
        rotationTick = tick;
    }

    @FunctionalInterface
    public interface CredentialPublisher { void publish(IssuedCredential credential) throws IOException; }

    /** Only valid when the publisher has evidence that no descriptor replacement occurred. */
    public static final class PublicationNotCommittedException extends IOException {
        private static final long serialVersionUID = 1L;
        public PublicationNotCommittedException(String message, Throwable cause) { super(message, cause); }
    }

    private void publish(CredentialPublisher publisher, IssuedCredential credential) throws IOException {
        try {
            publisher.publish(credential);
        } catch (PublicationNotCommittedException definiteFailure) {
            throw definiteFailure;
        } catch (IOException | RuntimeException | Error uncertainFailure) {
            revoke();
            throw uncertainFailure;
        }
    }

    public synchronized boolean validate(String token, UUID requestedInstance, String requestedPrincipal,
                                         String requiredScope, UUID credentialId, long generation) {
        if (requiredScope == null) return false;
        return authenticate(token).filter(claims -> claims.instanceId().equals(requestedInstance)
                && claims.osPrincipal().equals(requestedPrincipal) && claims.scopes().contains(requiredScope)
                && claims.credentialId().equals(credentialId) && claims.generation() == generation).isPresent();
    }

    /** Returns only server-owned bindings, never a claim supplied by a bearer caller. */
    public synchronized Optional<Claims> authenticate(String token) {
        if (revoked || !validTokenShape(token)) return Optional.empty();
        Instant now = now();
        long tick = ticker.getAsLong();
        purgeExpiredAt(now, tick);
        byte[] hash = digest(token);
        if (matchesToken(current, hash, now, tick)) return Optional.of(claims(current));
        if (matchesToken(previous, hash, now, tick)) return Optional.of(claims(previous));
        return Optional.empty();
    }

    /** Intended to be called by a single managed lifecycle scheduler. */
    public synchronized void purgeExpired() {
        purgeExpiredAt(now(), ticker.getAsLong());
    }

    private void purgeExpiredAt(Instant now, long tick) {
        if (previousUntil != null && (!now.isBefore(previousUntil) || !now.isBefore(previous.expiresAt)
                || elapsed(tick, rotationTick, GRACE) || elapsed(tick, previous.issuedTick, TTL))) {
            previous = null;
            previousUntil = null;
        }
    }

    private Claims claims(Entry entry) {
        return new Claims(instanceId, osPrincipal, scopes, entry.id, entry.generation);
    }

    private boolean matchesToken(Entry entry, byte[] hash, Instant now, long tick) {
        return entry != null && !now.isBefore(entry.issuedAt) && now.isBefore(entry.expiresAt)
                && !elapsed(tick, entry.issuedTick, TTL)
                && MessageDigest.isEqual(entry.digest, hash);
    }

    private static boolean elapsed(long now, long start, Duration limit) {
        return now - start >= limit.toNanos();
    }

    public record Claims(UUID instanceId, String osPrincipal, Set<String> scopes,
                         UUID credentialId, long generation) {
        public Claims { scopes = Set.copyOf(scopes); }
    }

    public synchronized void revoke() {
        revoked = true;
        current = null;
        previous = null;
        previousUntil = null;
    }

    private Candidate create(long generation, Instant now, long tick) {
        byte[] tokenBytes = new byte[32];
        random.nextBytes(tokenBytes);
        String token = Base64.getUrlEncoder().withoutPadding().encodeToString(tokenBytes);
        byte[] hash = digest(token);
        if (current != null && MessageDigest.isEqual(current.digest, hash)) {
            throw new IllegalStateException("Credential generation collision");
        }
        byte[] idBytes = new byte[16];
        random.nextBytes(idBytes);
        idBytes[6] = (byte) ((idBytes[6] & 0x0f) | 0x40);
        idBytes[8] = (byte) ((idBytes[8] & 0x3f) | 0x80);
        java.nio.ByteBuffer idBuffer = java.nio.ByteBuffer.wrap(idBytes);
        UUID id = new UUID(idBuffer.getLong(), idBuffer.getLong());
        if (current != null && current.id.equals(id)) throw new IllegalStateException("Credential ID collision");
        Instant expiresAt = now.plus(TTL);
        Entry entry = new Entry(id, generation, now, expiresAt, tick, hash);
        return new Candidate(entry, new IssuedCredential(id, generation, now, expiresAt, token));
    }

    private Instant now() {
        Instant observed = clock.instant().truncatedTo(ChronoUnit.MILLIS);
        if (lastObserved == null || observed.isAfter(lastObserved)) lastObserved = observed;
        return lastObserved;
    }

    private static boolean validTokenShape(String token) {
        if (token == null || token.length() != 43) return false;
        for (int i = 0; i < token.length(); i++) {
            char c = token.charAt(i);
            if (!(c >= 'A' && c <= 'Z') && !(c >= 'a' && c <= 'z')
                    && !(c >= '0' && c <= '9') && c != '-' && c != '_') return false;
        }
        return "AEIMQUYcgkosw048".indexOf(token.charAt(42)) >= 0;
    }

    private static byte[] digest(String token) {
        try {
            return MessageDigest.getInstance("SHA-256").digest(token.getBytes(java.nio.charset.StandardCharsets.UTF_8));
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 unavailable", e);
        }
    }

    private record Entry(UUID id, long generation, Instant issuedAt, Instant expiresAt, long issuedTick, byte[] digest) { }
    private record Candidate(Entry entry, IssuedCredential credential) { }

    public static final class IssuedCredential {
        private final UUID credentialId;
        private final long generation;
        private final Instant issuedAt;
        private final Instant expiresAt;
        private final String token;
        private IssuedCredential(UUID credentialId, long generation, Instant issuedAt, Instant expiresAt, String token) {
            this.credentialId = credentialId; this.generation = generation; this.issuedAt = issuedAt;
            this.expiresAt = expiresAt; this.token = token;
        }
        public UUID credentialId() { return credentialId; }
        public long generation() { return generation; }
        public Instant issuedAt() { return issuedAt; }
        public Instant expiresAt() { return expiresAt; }
        public String token() { return token; }
        @Override public String toString() { return "IssuedCredential[redacted]"; }
    }
}
