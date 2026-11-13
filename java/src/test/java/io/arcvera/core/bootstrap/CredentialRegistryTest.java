package io.arcvera.core.bootstrap;

import static org.junit.jupiter.api.Assertions.*;

import java.security.SecureRandom;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.util.Base64;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.concurrent.atomic.AtomicReference;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.Test;

class CredentialRegistryTest {
    private final MutableClock clock = new MutableClock();
    private final UUID instance = UUID.randomUUID();
    private final CredentialRegistry registry = new CredentialRegistry(clock, new SecureRandom(), clock::tick, instance, "user", Set.of("read"));

    private boolean valid(CredentialRegistry.IssuedCredential c) {
        return registry.validate(c.token(), instance, "user", "read", c.credentialId(), c.generation());
    }

    @Test void issueAndBinding() {
        var c = registry.issue();
        assertEquals(32, Base64.getUrlDecoder().decode(c.token()).length);
        assertEquals(43, c.token().length());
        assertTrue(valid(c));
        var claims = registry.authenticate(c.token()).orElseThrow();
        assertEquals(instance, claims.instanceId());
        assertEquals("user", claims.osPrincipal());
        assertEquals(Set.of("read"), claims.scopes());
        assertEquals(c.credentialId(), claims.credentialId());
        assertEquals(c.generation(), claims.generation());
        assertTrue(registry.authenticate("unknown").isEmpty());
        assertFalse(registry.validate(c.token(), UUID.randomUUID(), "user", "read", c.credentialId(), c.generation()));
        assertFalse(registry.validate(c.token(), instance, "other", "read", c.credentialId(), c.generation()));
        assertFalse(registry.validate(c.token(), instance, "user", "write", c.credentialId(), c.generation()));
        assertFalse(registry.validate(c.token(), instance, "user", "read", UUID.randomUUID(), c.generation()));
        assertFalse(registry.validate(c.token(), instance, "user", "read", c.credentialId(), 1));
        assertFalse(registry.validate("unknown", instance, "user", "read", c.credentialId(), c.generation()));
        assertFalse(registry.validate("A".repeat(42) + "B", instance, "user", "read", c.credentialId(), c.generation()));
        assertFalse(registry.validate("A".repeat(42) + "=", instance, "user", "read", c.credentialId(), c.generation()));
        assertThrows(IllegalStateException.class, registry::issue);
        assertFalse(c.toString().contains(c.token()));
        assertFalse(registry.toString().contains(c.token()));
    }

    @Test void expiryIsExclusive() {
        var c = registry.issue();
        clock.set(c.expiresAt().minusMillis(1));
        assertTrue(valid(c));
        clock.set(c.expiresAt());
        assertFalse(valid(c));
        clock.set(c.expiresAt().minusSeconds(1));
        assertFalse(valid(c));
        assertThrows(IllegalStateException.class, () -> registry.rotate(c.credentialId(), c.generation()));
    }

    @Test void rotationAndGraceBoundaries() {
        var old = registry.issue();
        clock.set(old.expiresAt().minusSeconds(120).minusMillis(1));
        assertThrows(IllegalStateException.class, () -> registry.rotate(old.credentialId(), old.generation()));
        clock.set(old.expiresAt().minusSeconds(120));
        var next = registry.rotate(old.credentialId(), old.generation());
        assertEquals(1, next.generation());
        assertNotEquals(old.credentialId(), next.credentialId());
        assertNotEquals(old.token(), next.token());
        assertTrue(valid(old));
        assertTrue(valid(next));
        assertThrows(IllegalStateException.class, () -> registry.rotate(old.credentialId(), old.generation()));
        clock.set(next.issuedAt().plusSeconds(30).minusMillis(1));
        assertTrue(valid(old));
        clock.set(next.issuedAt().plusSeconds(30));
        registry.purgeExpired();
        assertFalse(valid(old));
        assertTrue(registry.authenticate(old.token()).isEmpty());
        assertTrue(valid(next));
    }

    @Test void revokeImmediatelyRejectsBothGenerations() {
        var old = registry.issue();
        clock.set(old.expiresAt().minusSeconds(120));
        var next = registry.rotate(old.credentialId(), old.generation());
        registry.revoke();
        assertFalse(valid(old));
        assertFalse(valid(next));
        assertTrue(registry.authenticate(next.token()).isEmpty());
        assertThrows(IllegalStateException.class, registry::issue);
        assertThrows(IllegalStateException.class, () -> registry.rotate(next.credentialId(), next.generation()));
    }

    @Test void monotonicTimeLimitsValidityDespiteWallClockRollback() {
        var c = registry.issue();
        clock.advanceTick(java.time.Duration.ofMinutes(15));
        assertFalse(valid(c));
    }

    @Test void monotonicTimeAllowsRotationWhenWallClockStops() {
        var c = registry.issue();
        clock.advanceTick(java.time.Duration.ofMinutes(13));
        var next = registry.rotate(c.credentialId(), c.generation());
        assertEquals(1, next.generation());
        assertTrue(valid(next));
    }

    @Test void graceCannotBeExtendedByWallClockRollback() {
        var old = registry.issue();
        clock.set(old.expiresAt().minusSeconds(120));
        var next = registry.rotate(old.credentialId(), old.generation());
        clock.set(next.issuedAt().minusSeconds(60));
        clock.advanceTick(java.time.Duration.ofSeconds(30));
        assertFalse(valid(old));
        assertTrue(valid(next));
    }

    @Test void absoluteExpiryAppliesEvenWhenTickerHasNotAdvanced() {
        var c = registry.issue();
        clock.setWallOnly(c.expiresAt());
        assertFalse(valid(c));
    }

    @Test void competingRotationsHaveOneWinner() throws Exception {
        var old = registry.issue();
        clock.set(old.expiresAt().minusSeconds(120));
        var start = new CountDownLatch(1);
        try (var executor = Executors.newFixedThreadPool(2)) {
            var a = executor.submit(() -> rotateAfter(start, old));
            var b = executor.submit(() -> rotateAfter(start, old));
            start.countDown();
            assertNotEquals(a.get(), b.get());
        }
    }

    @Test void definitelyUnpublishedFailureLeavesStateUnchanged() throws Exception {
        var rejected = new AtomicReference<CredentialRegistry.IssuedCredential>();
        assertThrows(CredentialRegistry.PublicationNotCommittedException.class, () ->
                registry.issueAndPublish(candidate -> {
                    rejected.set(candidate);
                    throw new CredentialRegistry.PublicationNotCommittedException("not published", null);
                }));
        assertFalse(valid(rejected.get()));
        var old = registry.issueAndPublish(candidate -> { });
        clock.set(old.expiresAt().minusSeconds(120));
        assertThrows(CredentialRegistry.PublicationNotCommittedException.class, () ->
                registry.rotateAndPublish(old.credentialId(), old.generation(), candidate -> {
                    rejected.set(candidate);
                    throw new CredentialRegistry.PublicationNotCommittedException("not published", null);
                }));
        assertTrue(valid(old));
        assertFalse(valid(rejected.get()));
        var next = registry.rotateAndPublish(old.credentialId(), old.generation(), candidate -> { });
        assertTrue(valid(next));
        assertTrue(valid(old));
    }

    @Test void publishedThenUncertainFailureRevokesBoth() throws Exception {
        var old = registry.issue();
        clock.set(old.expiresAt().minusSeconds(120));
        var next = new AtomicReference<CredentialRegistry.IssuedCredential>();
        Path directory = Files.createTempDirectory(Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap"), "publication-fixture-");
        Path file = directory.resolve("connection.fixture");
        try {
            assertThrows(IOException.class, () -> registry.rotateAndPublish(old.credentialId(), old.generation(), candidate -> {
                next.set(candidate);
                Files.writeString(file, candidate.token());
                throw new IOException("publication outcome unknown");
            }));
            assertEquals(next.get().token(), Files.readString(file));
            assertFalse(valid(old));
            assertFalse(valid(next.get()));
            assertThrows(IllegalStateException.class, registry::issue);
        } finally {
            Files.deleteIfExists(file);
            Files.deleteIfExists(directory);
        }
    }

    @Test void uncertainInitialPublicationPermanentlyRevokesRegistry() {
        var candidate = new AtomicReference<CredentialRegistry.IssuedCredential>();
        assertThrows(IOException.class, () -> registry.issueAndPublish(value -> {
            candidate.set(value);
            throw new IOException("publication outcome unknown");
        }));
        assertFalse(valid(candidate.get()));
        assertThrows(IllegalStateException.class, registry::issue);
    }

    @Test void authenticationWaitsForPublication() throws Exception {
        var old = registry.issue();
        clock.set(old.expiresAt().minusSeconds(120));
        var candidate = new AtomicReference<CredentialRegistry.IssuedCredential>();
        var inside = new CountDownLatch(1);
        var release = new CountDownLatch(1);
        var authStarted = new CountDownLatch(1);
        try (var executor = Executors.newFixedThreadPool(2)) {
            var rotation = executor.submit(() -> registry.rotateAndPublish(old.credentialId(), old.generation(), value -> {
                candidate.set(value);
                inside.countDown();
                try {
                    if (!release.await(5, TimeUnit.SECONDS)) throw new IOException("publication test deadline");
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    throw new IOException("publication test interrupted", e);
                }
            }));
            assertTrue(inside.await(5, TimeUnit.SECONDS));
            var authentication = executor.submit(() -> { authStarted.countDown(); return valid(candidate.get()); });
            assertTrue(authStarted.await(5, TimeUnit.SECONDS));
            try {
                assertThrows(TimeoutException.class, () -> authentication.get(100, TimeUnit.MILLISECONDS));
            } finally {
                release.countDown();
            }
            assertEquals(1, rotation.get().generation());
            assertTrue(authentication.get());
        }
    }

    private boolean rotateAfter(CountDownLatch start, CredentialRegistry.IssuedCredential old) throws InterruptedException {
        start.await();
        try {
            return registry.rotate(old.credentialId(), old.generation()).generation() == 1;
        } catch (IllegalStateException expected) {
            return false;
        }
    }

    private static final class MutableClock extends Clock {
        private final AtomicReference<Instant> value = new AtomicReference<>(Instant.parse("2026-01-01T00:00:00Z"));
        private final AtomicLong ticker = new AtomicLong();
        void set(Instant instant) {
            Instant old = value.getAndSet(instant);
            if (instant.isAfter(old)) ticker.addAndGet(java.time.Duration.between(old, instant).toNanos());
        }
        void setWallOnly(Instant instant) { value.set(instant); }
        void advanceTick(java.time.Duration duration) { ticker.addAndGet(duration.toNanos()); }
        long tick() { return ticker.get(); }
        @Override public ZoneId getZone() { return ZoneOffset.UTC; }
        @Override public Clock withZone(ZoneId zone) { throw new UnsupportedOperationException(); }
        @Override public Instant instant() { return value.get(); }
    }
}
