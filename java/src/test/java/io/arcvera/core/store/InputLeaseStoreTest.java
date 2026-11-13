package io.arcvera.core.store;

import static org.junit.jupiter.api.Assertions.*;

import io.arcvera.core.lease.InputLeaseStore;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.time.ZoneOffset;
import java.util.List;
import java.util.Map;
import java.util.TimeZone;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.TestInstance;
import org.springframework.boot.SpringBootConfiguration;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.autoconfigure.EnableAutoConfiguration;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.core.env.MapPropertySource;
import org.springframework.data.jdbc.repository.config.EnableJdbcRepositories;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.transaction.PlatformTransactionManager;
import org.springframework.transaction.support.TransactionTemplate;
import com.zaxxer.hikari.HikariDataSource;

@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class InputLeaseStoreTest {
    private static final Instant BASE = Instant.parse("2026-03-08T07:30:00.123Z");
    private final String schema = "core_lease_" + UUID.randomUUID().toString().replace("-", "");
    private ConfigurableApplicationContext context;
    private HikariDataSource pool;
    private JdbcClient jdbc;
    private InputLeaseStore leases;
    private TaskAcceptanceStore tasks;
    private MutableClock clock;
    private boolean created;

    @SpringBootConfiguration
    @EnableAutoConfiguration
    @Import({TaskAcceptanceStore.class, AcceptanceTransaction.class, OperationIdentityService.class, InputLeaseStore.class})
    @EnableJdbcRepositories(basePackageClasses = ConversationRepository.class)
    static class TestApplication {
        @Bean Clock clock() { return new MutableClock(); }
        @Bean TaskAcceptanceStore.Retention retention() {
            return new TaskAcceptanceStore.Retention(Duration.ofHours(1), Duration.ofMinutes(1), Duration.ofMinutes(5), Duration.ofHours(2));
        }
        @Bean InputLeaseStore.Policy leasePolicy() {
            return new InputLeaseStore.Policy(Duration.ofSeconds(10), Duration.ofHours(1), Duration.ofMinutes(1), Duration.ofMinutes(5), Duration.ofHours(2));
        }
    }
    static final class MutableClock extends Clock {
        final AtomicReference<Instant> value = new AtomicReference<>(BASE);
        @Override public ZoneId getZone() { return ZoneOffset.UTC; }
        @Override public Clock withZone(ZoneId zone) { return Clock.fixed(value.get(), zone); }
        @Override public Instant instant() { return value.get(); }
    }
    private String required(String key) {
        String value = System.getenv(key);
        if (value == null || value.isBlank()) throw new IllegalStateException("missing " + key);
        return value;
    }
    private String scopedUrl() {
        String base = required("ARCVERA_TEST_DB_URL");
        return base + (base.contains("?") ? "&" : "?") + "currentSchema=" + schema;
    }
    @BeforeAll void setup() throws Exception {
        boolean ready = false;
        try {
            context = new SpringApplicationBuilder(TestApplication.class).web(WebApplicationType.NONE)
                    .properties(Map.of("spring.datasource.url", "jdbc:postgresql://127.0.0.1:1/invalid", "spring.datasource.hikari.maximum-pool-size", "1"))
                    .initializers(app -> {
                        try {
                            String password = Files.readString(Path.of(required("ARCVERA_TEST_DB_PASSWORD_FILE"))).strip();
                            app.getEnvironment().getPropertySources().addFirst(new MapPropertySource("isolatedLeaseDatabase", Map.of(
                                    "spring.datasource.url", scopedUrl(), "spring.datasource.username", required("ARCVERA_TEST_DB_USER"),
                                    "spring.datasource.password", password, "spring.datasource.type", "com.zaxxer.hikari.HikariDataSource",
                                    "spring.datasource.hikari.maximum-pool-size", "4", "spring.main.banner-mode", "off", "logging.level.root", "ERROR")));
                        } catch (java.io.IOException failure) { throw new java.io.UncheckedIOException(failure); }
                    }).run();
            pool = context.getBean(HikariDataSource.class);
            assertEquals(scopedUrl(), pool.getJdbcUrl()); assertEquals(4, pool.getMaximumPoolSize());
            jdbc = context.getBean(JdbcClient.class);
            jdbc.sql("CREATE SCHEMA " + schema).update(); created = true;
            tasks = context.getBean(TaskAcceptanceStore.class); tasks.initialize();
            leases = context.getBean(InputLeaseStore.class); leases.initialize();
            clock = (MutableClock) context.getBean(Clock.class); ready = true;
        } finally { if (!ready) cleanup(); }
    }
    @AfterAll void cleanup() {
        if (context == null || !context.isActive()) return;
        try {
            if (created) {
                if (!schema.matches("core_lease_[0-9a-f]{32}")) throw new IllegalStateException("unexpected schema");
                jdbc.sql("DROP SCHEMA " + schema + " CASCADE").update();
                assertEquals(0, jdbc.sql("SELECT COUNT(*) FROM information_schema.schemata WHERE schema_name=:schema").param("schema", schema).query(Integer.class).single());
            }
        } finally { context.close(); }
        if (pool != null) assertTrue(pool.isClosed());
    }
    private UUID conversation() { UUID id = UUID.randomUUID(); tasks.createConversation(id); return id; }
    private InputLeaseStore.Request acquire(UUID conversation, UUID client, UUID operation) {
        return new InputLeaseStore.Request("alice", operation, clock.instant(), "digest", conversation, 0, client, InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.GUI);
    }
    private InputLeaseStore.Request change(UUID conversation, InputLeaseStore.Lease lease, InputLeaseStore.Action action) {
        return new InputLeaseStore.Request("alice", UUID.randomUUID(), clock.instant(), "change", conversation, lease.revision(), lease.clientInstanceId(), action, lease.leaseId(), null);
    }
    private InputLeaseStore.Proof proof(InputLeaseStore.Lease lease) {
        return new InputLeaseStore.Proof(lease.leaseId(), lease.revision(), lease.clientInstanceId());
    }
    @Test void lifecycleReplayAndProofBinding() {
        clock.value.set(BASE); UUID conversation = conversation(); UUID client = UUID.randomUUID();
        assertTrue(leases.query(conversation).isEmpty());
        var request = acquire(conversation, client, UUID.randomUUID());
        var first = leases.change(request);
        assertEquals(1, first.revision()); assertEquals(BASE.plusSeconds(10), first.expiresAt());
        assertEquals(first, leases.change(request));
        var changedPayload = new InputLeaseStore.Request("alice", request.operationId(), BASE, "other", conversation, 0, client, InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.GUI);
        assertThrows(InputLeaseStore.Conflict.class, () -> leases.change(changedPayload));
        for (var changed : List.of(
                new InputLeaseStore.Request("alice", request.operationId(), BASE.minusMillis(1), "digest", conversation, 0, client, InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.GUI),
                new InputLeaseStore.Request("alice", request.operationId(), BASE, "digest", conversation, 0, UUID.randomUUID(), InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.GUI),
                new InputLeaseStore.Request("alice", request.operationId(), BASE, "digest", conversation, 1, client, InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.GUI),
                new InputLeaseStore.Request("alice", request.operationId(), BASE, "digest", conversation, 0, client, InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.TERMINAL)))
            assertThrows(InputLeaseStore.Conflict.class, () -> leases.change(changed));
        var renewed = leases.change(change(conversation, first, InputLeaseStore.Action.RENEW));
        assertEquals(2, renewed.revision());
        var stale = new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", BASE, "task", conversation, 0, proof(first));
        assertThrows(InputLeaseStore.Conflict.class, () -> tasks.acceptTask(stale));
        var valid = new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", BASE, "task", conversation, 0, proof(renewed));
        var receipt = tasks.acceptTask(valid); assertEquals(receipt, tasks.acceptTask(valid));
        assertThrows(TaskAcceptanceStore.Conflict.class, () -> tasks.acceptTask(new TaskAcceptanceStore.Request("alice", request.operationId(), "task", BASE, "digest", conversation, 0, proof(renewed))));
        assertThrows(InputLeaseStore.Conflict.class, () -> leases.change(new InputLeaseStore.Request("alice", valid.operationId(), BASE, "task", conversation, renewed.revision(), client, InputLeaseStore.Action.RELEASE, renewed.leaseId(), null)));
        var released = leases.change(change(conversation, renewed, InputLeaseStore.Action.RELEASE));
        assertEquals(InputLeaseStore.State.RELEASED, released.state());
        assertThrows(InputLeaseStore.Conflict.class, () -> leases.change(change(conversation, renewed, InputLeaseStore.Action.RENEW)));
        var next = leases.change(new InputLeaseStore.Request("alice", UUID.randomUUID(), BASE, "next", conversation, released.revision(), UUID.randomUUID(), InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.TERMINAL));
        assertNotEquals(first.leaseId(), next.leaseId()); assertEquals(released.revision() + 1, next.revision());
    }
    @Test void expiryRollbackAndNewGeneration() {
        clock.value.set(BASE); UUID conversation = conversation(); var first = leases.change(acquire(conversation, UUID.randomUUID(), UUID.randomUUID()));
        clock.value.set(BASE.plusSeconds(10));
        assertEquals(InputLeaseStore.State.EXPIRED, leases.query(conversation).orElseThrow().state());
        assertThrows(InputLeaseStore.Conflict.class, () -> leases.change(change(conversation, first, InputLeaseStore.Action.RENEW)));
        clock.value.set(BASE.plusSeconds(9));
        assertThrows(InputLeaseStore.Conflict.class, () -> leases.query(conversation));
        var recreated = new InputLeaseStore(jdbc, context.getBean(OperationIdentityService.class), context.getBean(PlatformTransactionManager.class), clock, context.getBean(InputLeaseStore.Policy.class));
        assertThrows(InputLeaseStore.Conflict.class, () -> recreated.query(conversation));
        clock.value.set(BASE.plusSeconds(10));
        var next = leases.change(new InputLeaseStore.Request("alice", UUID.randomUUID(), clock.instant(), "new", conversation, first.revision(), UUID.randomUUID(), InputLeaseStore.Action.ACQUIRE, null, InputLeaseStore.Mode.TERMINAL));
        assertNotEquals(first.leaseId(), next.leaseId()); assertEquals(2, next.revision());
        var old = new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", clock.instant(), "old", conversation, 0, proof(first));
        assertThrows(InputLeaseStore.Conflict.class, () -> tasks.acceptTask(old));
        clock.value.set(BASE);
    }
    @Test void competingAcquiresHaveOneWinner() throws Exception {
        clock.value.set(BASE); UUID conversation = conversation(); var latch = new CountDownLatch(1);
        try (var executor = Executors.newFixedThreadPool(2)) {
            var requests = List.of(acquire(conversation, UUID.randomUUID(), UUID.randomUUID()), acquire(conversation, UUID.randomUUID(), UUID.randomUUID()));
            var futures = requests.stream().map(request -> executor.submit(() -> {
                latch.await(); try { leases.change(request); return true; } catch (InputLeaseStore.Conflict expected) { return false; }
            })).toList();
            latch.countDown(); int winners = 0; for (var future : futures) if (future.get(10, TimeUnit.SECONDS)) winners++;
            assertEquals(1, winners);
        }
        assertEquals(1, jdbc.sql("SELECT COUNT(*) FROM input_lease WHERE conversation_id=:id").param("id", conversation.toString()).query(Integer.class).single());
        assertEquals(1, jdbc.sql("SELECT COUNT(*) FROM lease_receipt WHERE conversation_id=:id").param("id", conversation.toString()).query(Integer.class).single());
    }
    @Test void databaseFailureRollsBackIdentityAndLease() {
        clock.value.set(BASE); UUID conversation = conversation(); var request = acquire(conversation, UUID.randomUUID(), UUID.randomUUID());
        jdbc.sql("ALTER TABLE lease_receipt ADD CONSTRAINT lease_test_failure CHECK (conversation_id <> '" + conversation + "')").update();
        try {
            assertThrows(org.springframework.dao.DataIntegrityViolationException.class, () -> leases.change(request));
            assertEquals(0, jdbc.sql("SELECT COUNT(*) FROM input_lease WHERE conversation_id=:id").param("id", conversation.toString()).query(Integer.class).single());
            assertEquals(0, jdbc.sql("SELECT COUNT(*) FROM operation_identity WHERE operation_id=:id").param("id", request.operationId().toString()).query(Integer.class).single());
        } finally { jdbc.sql("ALTER TABLE lease_receipt DROP CONSTRAINT lease_test_failure").update(); }
    }
    @Test void reservationOutsideTransactionIsRejected() {
        var identities = context.getBean(OperationIdentityService.class);
        assertThrows(IllegalStateException.class, () -> identities.reserve("alice", UUID.randomUUID(), "task", BASE, "digest"));
    }
    @Test void duplicateConcurrentRequestsReplayOneReceipt() throws Exception {
        clock.value.set(BASE); UUID conversation = conversation(); var request = acquire(conversation, UUID.randomUUID(), UUID.randomUUID());
        var latch = new CountDownLatch(1);
        try (var executor = Executors.newFixedThreadPool(2)) {
            var first = executor.submit(() -> { latch.await(); return leases.change(request); });
            var second = executor.submit(() -> { latch.await(); return leases.change(request); });
            latch.countDown(); assertEquals(first.get(10, TimeUnit.SECONDS), second.get(10, TimeUnit.SECONDS));
        }
        assertEquals(1, jdbc.sql("SELECT COUNT(*) FROM lease_receipt WHERE conversation_id=:id").param("id", conversation.toString()).query(Integer.class).single());
    }
    @Test void wrongProofCannotAcceptTaskAndExpiryDoesNotCancelAcceptedTask() {
        clock.value.set(BASE); UUID conversation = conversation(); var first = leases.change(acquire(conversation, UUID.randomUUID(), UUID.randomUUID()));
        for (var candidate : List.of(new InputLeaseStore.Proof(UUID.randomUUID(), first.revision(), first.clientInstanceId()),
                new InputLeaseStore.Proof(first.leaseId(), first.revision() + 1, first.clientInstanceId()),
                new InputLeaseStore.Proof(first.leaseId(), first.revision(), UUID.randomUUID()))) {
            var request = new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", BASE, "input", conversation, 0, candidate);
            assertThrows(InputLeaseStore.Conflict.class, () -> tasks.acceptTask(request));
        }
        var otherPrincipal = new TaskAcceptanceStore.Request("bob", UUID.randomUUID(), "task", BASE, "input", conversation, 0, proof(first));
        assertThrows(InputLeaseStore.Conflict.class, () -> tasks.acceptTask(otherPrincipal));
        var otherConversation = new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", BASE, "input", conversation(), 0, proof(first));
        assertThrows(InputLeaseStore.Conflict.class, () -> tasks.acceptTask(otherConversation));
        var accepted = tasks.acceptTask(new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", BASE, "input", conversation, 0, proof(first)));
        clock.value.set(BASE.plusSeconds(10));
        assertEquals(InputLeaseStore.State.EXPIRED, leases.query(conversation).orElseThrow().state());
        var snapshot = tasks.snapshot(conversation);
        assertEquals(accepted.taskId(), snapshot.activeTaskId()); assertEquals("QUEUED", snapshot.activeState());
        clock.value.set(BASE);
    }
    @Test void expiredWhileWaitingForLockIsRejectedAndObservationSurvivesRollback() throws Exception {
        clock.value.set(BASE); UUID conversation = conversation(); var first = leases.change(acquire(conversation, UUID.randomUUID(), UUID.randomUUID()));
        var locked = new CountDownLatch(1); var release = new CountDownLatch(1); var started = new CountDownLatch(1);
        var tx = new TransactionTemplate(context.getBean(PlatformTransactionManager.class));
        try (var executor = Executors.newFixedThreadPool(2)) {
            var blocker = executor.submit(() -> tx.execute(status -> {
                leases.lockConversation(conversation); locked.countDown();
                try { if (!release.await(10, TimeUnit.SECONDS)) throw new IllegalStateException("release timeout"); }
                catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); throw new IllegalStateException(interrupted); }
                return true;
            }));
            assertTrue(locked.await(10, TimeUnit.SECONDS));
            var candidate = executor.submit(() -> {
                started.countDown();
                try {
                    tasks.acceptTask(new TaskAcceptanceStore.Request("alice", UUID.randomUUID(), "task", BASE, "input", conversation, 0, proof(first)));
                    return false;
                } catch (InputLeaseStore.Conflict expected) { return true; }
            });
            try {
                assertTrue(started.await(10, TimeUnit.SECONDS));
                assertThrows(java.util.concurrent.TimeoutException.class, () -> candidate.get(250, TimeUnit.MILLISECONDS));
                clock.value.set(BASE.plusSeconds(10));
            } finally { release.countDown(); }
            assertTrue(blocker.get(10, TimeUnit.SECONDS)); assertTrue(candidate.get(10, TimeUnit.SECONDS));
        }
        clock.value.set(BASE.plusSeconds(9));
        assertThrows(InputLeaseStore.Conflict.class, () -> leases.query(conversation));
        assertNull(tasks.snapshot(conversation).activeTaskId()); clock.value.set(BASE);
    }
    @Test void utcCalendarFieldsSurviveJvmDstGapAndFold() {
        TimeZone original = TimeZone.getDefault();
        try {
            TimeZone.setDefault(TimeZone.getTimeZone("America/New_York"));
            for (Instant time : List.of(Instant.parse("2026-03-08T02:30:00.123Z"), Instant.parse("2026-11-01T01:30:00.123Z"))) {
                clock.value.set(time); UUID conversation = conversation(); var lease = leases.change(acquire(conversation, UUID.randomUUID(), UUID.randomUUID()));
                LocalDateTime fields = jdbc.sql("SELECT expires_at FROM input_lease WHERE conversation_id=:id").param("id", conversation.toString())
                        .query((rs, n) -> rs.getObject(1, LocalDateTime.class)).single();
                assertEquals(lease.expiresAt(), fields.toInstant(ZoneOffset.UTC));
                assertEquals(time.plusSeconds(10), lease.expiresAt());
            }
        } finally { TimeZone.setDefault(original); clock.value.set(BASE); }
    }
}
