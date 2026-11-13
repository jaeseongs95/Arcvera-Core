package io.arcvera.core.bootstrap;

import static org.junit.jupiter.api.Assertions.*;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class WindowsProfileLockTest {
    @Test void protectedProfileHasExclusiveLockAndRejectsHardLinks() throws Exception {
        Path root = Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap");
        Path home = Files.createTempDirectory(root, "lock-fixture-");
        UUID profile = UUID.randomUUID();
        Path directory = home.resolve("runtime/core2").resolve(profile.toString());
        Path lockFile = directory.resolve("profile.lock");
        Path alias = directory.resolve("alias.lock");
        try {
            try (var lock = new WindowsProfileLock(home, profile)) {
                assertEquals(directory, lock.directory());
                assertTrue(lock.sid().startsWith("S-1-"));
                assertThrows(IOException.class, () -> new WindowsProfileLock(home, profile));
            }
            try (var reacquired = new WindowsProfileLock(home, profile)) {
                assertEquals(directory, reacquired.directory());
                assertTrue(Files.exists(lockFile));
            }
            Files.createLink(alias, lockFile);
            assertThrows(IOException.class, () -> new WindowsProfileLock(home, profile));
            Files.delete(alias);
            try (var reacquired = new WindowsProfileLock(home, profile)) {
                assertEquals(directory, reacquired.directory());
                assertTrue(Files.exists(lockFile));
            }
        } finally {
            Files.deleteIfExists(alias);
            Files.deleteIfExists(lockFile);
            Files.deleteIfExists(directory);
            Files.deleteIfExists(home.resolve("runtime/core2"));
            Files.deleteIfExists(home.resolve("runtime"));
            Files.deleteIfExists(home);
        }
    }
}
