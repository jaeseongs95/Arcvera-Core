package io.arcvera.core.bootstrap;

import static org.junit.jupiter.api.Assertions.*;

import java.security.SecureRandom;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.scheduling.annotation.ScheduledAnnotationBeanPostProcessor;
import org.springframework.scheduling.concurrent.ThreadPoolTaskScheduler;
import org.springframework.security.oauth2.server.resource.introspection.BadOpaqueTokenException;

class LocalCredentialIntrospectorTest {
    @Test void bearerUsesServerOwnedClaimsAndRejectsInvalidTokens() {
        var ticker = new AtomicLong();
        var instance = UUID.randomUUID();
        var registry = new CredentialRegistry(Clock.fixed(Instant.parse("2026-01-01T00:00:00Z"), ZoneOffset.UTC),
                new SecureRandom(), ticker::get, instance, "os-user", Set.of("read"));
        var c = registry.issue();
        var adapter = new LocalCredentialIntrospector(registry);
        var principal = adapter.introspect(c.token());
        assertEquals("os-user", principal.getName());
        assertEquals(instance.toString(), principal.getAttribute("instanceId"));
        assertEquals(c.credentialId().toString(), principal.getAttribute("credentialId"));
        assertEquals(c.generation(), ((Long) principal.getAttribute("generation")).longValue());
        assertTrue(principal.getAuthorities().stream().anyMatch(a -> a.getAuthority().equals("SCOPE_read")));
        var error = assertThrows(BadOpaqueTokenException.class, () -> adapter.introspect("unknown"));
        assertFalse(error.toString().contains(c.token()));
        ticker.set(java.time.Duration.ofMinutes(15).toNanos());
        assertThrows(BadOpaqueTokenException.class, () -> adapter.introspect(c.token()));
        registry.revoke();
        assertThrows(BadOpaqueTokenException.class, () -> adapter.introspect(c.token()));
    }

    @Test void contextOwnsOneCleanupAndStopsScheduler() throws InterruptedException {
        var context = new AnnotationConfigApplicationContext();
        var ran = new CountDownLatch(1);
        Clock clock = new Clock() {
            @Override public ZoneId getZone() { return ZoneOffset.UTC; }
            @Override public Clock withZone(ZoneId zone) { throw new UnsupportedOperationException(); }
            @Override public Instant instant() { ran.countDown(); return Instant.parse("2026-01-01T00:00:00Z"); }
        };
        var registry = new CredentialRegistry(clock,
                new SecureRandom(), System::nanoTime, UUID.randomUUID(), "os-user", Set.of("read"));
        context.registerBean(CredentialRegistry.class, () -> registry);
        context.register(BootstrapSchedulingConfiguration.class, CredentialCleanup.class, LocalCredentialIntrospector.class);
        context.refresh();
        var scheduler = context.getBean("bootstrapTaskScheduler", ThreadPoolTaskScheduler.class);
        var executor = scheduler.getScheduledThreadPoolExecutor();
        try {
            assertEquals(1, context.getBeansOfType(CredentialCleanup.class).size());
            assertEquals(1, context.getBean(ScheduledAnnotationBeanPostProcessor.class).getScheduledTasks().size());
            assertEquals(1, scheduler.getPoolSize());
            assertFalse(executor.isShutdown());
            assertTrue(ran.await(5, TimeUnit.SECONDS), "scheduled cleanup did not run");
        } finally {
            context.close();
        }
        assertTrue(executor.isShutdown());
        assertTrue(executor.getQueue().isEmpty());
    }
}
