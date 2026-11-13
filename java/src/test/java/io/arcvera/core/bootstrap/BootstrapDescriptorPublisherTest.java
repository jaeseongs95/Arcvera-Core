package io.arcvera.core.bootstrap;

import static org.junit.jupiter.api.Assertions.*;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.SecureRandom;
import java.time.Clock;
import java.time.Duration;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.Test;
import tools.jackson.databind.json.JsonMapper;

class BootstrapDescriptorPublisherTest {
    @Test void actualProtectedPublicationAndTamperFailure() throws Exception {
        try (var fixture = new Fixture()) {
            var first = fixture.publisher.issue();
            var firstDocument = fixture.mapper.readTree(Files.readAllBytes(fixture.file));
            assertTrue(first.token().equals(firstDocument.get("credential").asString()));
            assertEquals(fixture.instance.toString(), firstDocument.get("instanceId").asString());
            assertEquals(fixture.profile.toString(), firstDocument.get("profileId").asString());
            assertEquals(fixture.epoch.toString(), firstDocument.get("storeEpoch").asString());
            assertEquals(fixture.lock.sid(), firstDocument.get("osUserSid").asString());
            assertEquals("https", firstDocument.get("endpoint").get("scheme").asString());
            assertEquals("127.0.0.1", firstDocument.get("endpoint").get("host").asString());
            assertEquals(fixture.tls.spkiSha256(), firstDocument.get("endpoint").get("serverSpkiSha256").asString());
            assertTrue(fixture.registry.authenticate(first.token()).isPresent());
            fixture.ticker.set(Duration.ofMinutes(13).toNanos());
            var second = fixture.publisher.rotate(first.credentialId(), first.generation());
            var secondDocument = fixture.mapper.readTree(Files.readAllBytes(fixture.file));
            assertEquals("1", secondDocument.get("generation").asString());
            assertTrue(second.token().equals(secondDocument.get("credential").asString()));
            assertNotEquals(firstDocument.get("descriptorId").asString(), secondDocument.get("descriptorId").asString());
            assertTrue(fixture.registry.authenticate(second.token()).isPresent());
            Files.writeString(fixture.file, "tampered");
            fixture.ticker.addAndGet(Duration.ofMinutes(13).toNanos());
            assertThrows(CredentialRegistry.PublicationNotCommittedException.class,
                    () -> fixture.publisher.rotate(second.credentialId(), second.generation()));
            assertEquals("tampered", Files.readString(fixture.file));
            assertTrue(fixture.registry.authenticate(second.token()).isPresent());
        }
    }

    @Test void unknownExistingDescriptorIsNeverOverwritten() throws Exception {
        try (var fixture = new Fixture()) {
            Files.writeString(fixture.file, "foreign");
            assertThrows(CredentialRegistry.PublicationNotCommittedException.class, fixture.publisher::issue);
            assertEquals("foreign", Files.readString(fixture.file));
            assertNotNull(fixture.registry.issue());
        }
    }

    @Test void certificateExpiryCannotPrecedePublishedCredential() throws Exception {
        try (var fixture = new Fixture()) {
            var later = new CredentialRegistry(Clock.offset(fixture.clock, Duration.ofHours(23).plusMinutes(50)),
                    fixture.random, fixture.ticker::get, fixture.instance, fixture.lock.sid(), Set.of("read"));
            var publisher = new BootstrapDescriptorPublisher(later, fixture.lock, fixture.mapper, fixture.epoch, 12345, fixture.tls);
            assertThrows(CredentialRegistry.PublicationNotCommittedException.class, publisher::issue);
            assertFalse(Files.exists(fixture.file));
            assertNotNull(later.issue());
        }
    }

    @Test void hardLinkedDescriptorCannotBeReplaced() throws Exception {
        try (var fixture = new Fixture()) {
            var first = fixture.publisher.issue();
            Path alias = fixture.lock.directory().resolve("descriptor-alias");
            Files.createLink(alias, fixture.file);
            try {
                fixture.ticker.set(Duration.ofMinutes(13).toNanos());
                assertThrows(CredentialRegistry.PublicationNotCommittedException.class,
                        () -> fixture.publisher.rotate(first.credentialId(), first.generation()));
                assertTrue(fixture.registry.authenticate(first.token()).isPresent());
            } finally {
                Files.deleteIfExists(alias);
            }
            assertNotNull(fixture.publisher.rotate(first.credentialId(), first.generation()));
        }
    }

    @Test void closeRevokesAndWithdrawsOnlyUnchangedOwnedDescriptor() throws Exception {
        try (var fixture = new Fixture()) {
            var credential = fixture.publisher.issue();
            fixture.publisher.close();
            assertFalse(Files.exists(fixture.file));
            assertTrue(fixture.registry.authenticate(credential.token()).isEmpty());
            fixture.publisher.close();
        }
    }

    @Test void closeRefusesToDeleteModifiedDescriptorButStillRevokes() throws Exception {
        try (var fixture = new Fixture()) {
            var credential = fixture.publisher.issue();
            Files.writeString(fixture.file, "other generation");
            assertThrows(IOException.class, fixture.publisher::close);
            assertEquals("other generation", Files.readString(fixture.file));
            assertTrue(fixture.registry.authenticate(credential.token()).isEmpty());
        }
    }

    private static final class Fixture implements AutoCloseable {
        final Path home = Files.createTempDirectory(Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap"), "descriptor-fixture-");
        final UUID profile = UUID.randomUUID();
        final UUID instance = UUID.randomUUID();
        final UUID epoch = UUID.randomUUID();
        final AtomicLong ticker = new AtomicLong();
        final Clock clock = Clock.systemUTC();
        final SecureRandom random = new SecureRandom();
        final JsonMapper mapper = JsonMapper.builder().build();
        final WindowsProfileLock lock = new WindowsProfileLock(home, profile);
        final Path file = lock.directory().resolve("connection.json");
        final CredentialRegistry registry = new CredentialRegistry(clock, random, ticker::get, instance, lock.sid(), Set.of("read"));
        final LocalTlsMaterial tls = new LocalTlsMaterial(clock, random, "127.0.0.1");
        final BootstrapDescriptorPublisher publisher = new BootstrapDescriptorPublisher(registry, lock, mapper, epoch, 12345, tls);
        Fixture() throws Exception { }
        @Override public void close() throws IOException {
            lock.close();
            Files.deleteIfExists(file);
            Files.deleteIfExists(home.resolve("runtime/core2").resolve(profile.toString()).resolve("profile.lock"));
            Files.deleteIfExists(home.resolve("runtime/core2").resolve(profile.toString()));
            Files.deleteIfExists(home.resolve("runtime/core2"));
            Files.deleteIfExists(home.resolve("runtime"));
            Files.deleteIfExists(home);
        }
    }
}
