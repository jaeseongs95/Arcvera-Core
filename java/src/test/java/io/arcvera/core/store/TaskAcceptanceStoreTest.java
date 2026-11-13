package io.arcvera.core.store;

import static org.junit.jupiter.api.Assertions.*;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.io.UncheckedIOException;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.time.ZoneId;
import java.util.List;
import java.util.Map;
import java.util.TimeZone;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.TestInstance;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.boot.SpringBootConfiguration;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.autoconfigure.EnableAutoConfiguration;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.context.support.GenericApplicationContext;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.ComponentScan;
import org.springframework.core.env.MapPropertySource;
import org.springframework.data.jdbc.repository.config.EnableJdbcRepositories;
import org.springframework.transaction.PlatformTransactionManager;
import com.zaxxer.hikari.HikariDataSource;
import org.springframework.transaction.support.TransactionTemplate;

@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class TaskAcceptanceStoreTest {
    private final String schema = "core_p0_" + UUID.randomUUID().toString().replace("-", "");
    private final Path evidence = Path.of("build/reports/store-database-run.txt");
    private final Instant now = Instant.parse("2026-03-08T07:30:00Z");
    private JdbcClient admin;
    private JdbcClient jdbc;
    private TransactionTemplate tx;
    private TaskAcceptanceStore store;
    private ConfigurableApplicationContext context;
    private HikariDataSource pool;
    private boolean schemaCreated;
    private final TaskAcceptanceStore.Retention retention = new TaskAcceptanceStore.Retention(Duration.ofHours(1), Duration.ofMinutes(1), Duration.ofMinutes(5), Duration.ofHours(2));

    @BeforeAll void setup() throws Exception {
        boolean ready = false;
        try {
            context = new SpringApplicationBuilder(TestApplication.class).web(WebApplicationType.NONE)
                    .properties(Map.of("spring.datasource.url", "jdbc:postgresql://127.0.0.1:1/invalid", "spring.datasource.hikari.maximum-pool-size", "1"))
                    .initializers(application -> {
                        try {
                            String password = Files.readString(Path.of(required("ARCVERA_TEST_DB_PASSWORD_FILE"))).strip();
                            application.getEnvironment().getPropertySources().addFirst(new MapPropertySource("isolatedTestDatabase", Map.of(
                                    "spring.datasource.url", scopedUrl(), "spring.datasource.username", required("ARCVERA_TEST_DB_USER"),
                                    "spring.datasource.password", password, "spring.datasource.type", "com.zaxxer.hikari.HikariDataSource",
                                    "spring.datasource.hikari.maximum-pool-size", "4", "spring.main.banner-mode", "off", "logging.level.root", "ERROR")));
                        } catch (java.io.IOException failure) { throw new UncheckedIOException(failure); }
                    }).run();
            pool = context.getBean(HikariDataSource.class);
            assertEquals(scopedUrl(), pool.getJdbcUrl());
            assertEquals(4, pool.getMaximumPoolSize());
            admin = context.getBean(JdbcClient.class);
            admin.sql("CREATE SCHEMA " + schema).update();
            schemaCreated = true;
            String version = admin.sql("SHOW server_version").query(String.class).single();
            String timezone = admin.sql("SHOW timezone").query(String.class).single();
            String encoding = admin.sql("SHOW server_encoding").query(String.class).single();
            Files.createDirectories(evidence.getParent());
            Files.writeString(evidence, "schema=" + schema + "\nserver_version=" + version + "\ntimezone=" + timezone + "\nencoding=" + encoding + "\ncreate=success\n");
            jdbc = admin;
            tx = new TransactionTemplate(context.getBean(PlatformTransactionManager.class));
            store = context.getBean(TaskAcceptanceStore.class);
            store.initialize();
            ready = true;
        } finally { if (!ready) cleanup(); }
    }
    private String required(String name) {
        String value = System.getenv(name);
        if (value == null || value.isBlank()) throw new IllegalStateException("missing " + name);
        return value;
    }
    private String scopedUrl() {
        String base = required("ARCVERA_TEST_DB_URL");
        return base + (base.contains("?") ? "&" : "?") + "currentSchema=" + schema;
    }
    @SpringBootConfiguration
    @EnableAutoConfiguration
    @ComponentScan(basePackageClasses = TaskAcceptanceStore.class)
    @EnableJdbcRepositories(basePackageClasses = ConversationRepository.class)
    static class TestApplication {
        @Bean Clock clock() { return new MutableClock(Instant.parse("2026-03-08T07:30:00Z")); }
        @Bean TaskAcceptanceStore.Retention retention() {
            return new TaskAcceptanceStore.Retention(Duration.ofHours(1), Duration.ofMinutes(1), Duration.ofMinutes(5), Duration.ofHours(2));
        }
    }
    static final class MutableClock extends Clock {
        private Instant current;
        MutableClock(Instant initial) { current = initial; }
        void set(Instant value) { current = value; }
        @Override public ZoneId getZone() { return ZoneOffset.UTC; }
        @Override public Clock withZone(ZoneId zone) { return Clock.fixed(current, zone); }
        @Override public Instant instant() { return current; }
    }
    @AfterAll void cleanup() throws Exception {
        if (context == null || !context.isActive()) return;
        boolean[] removed = {false};
        try {
            cleanupAndClose(context, () -> {
                if (!schemaCreated || admin == null) return;
                if (!schema.matches("core_p0_[0-9a-f]{32}")) throw new IllegalStateException("unexpected schema name");
                int found = admin.sql("SELECT COUNT(*) FROM information_schema.schemata WHERE schema_name=:schema").param("schema", schema).query(Integer.class).single();
                if (found == 1) admin.sql("DROP SCHEMA " + schema + " CASCADE").update();
                int remaining = admin.sql("SELECT COUNT(*) FROM information_schema.schemata WHERE schema_name=:schema").param("schema", schema).query(Integer.class).single();
                if (remaining != 0) throw new IllegalStateException("test schema still exists");
                schemaCreated = false; removed[0] = true;
            });
        } finally {
            if (pool != null && !pool.isClosed()) throw new IllegalStateException("pool still open");
        }
        if (removed[0]) Files.writeString(evidence, "cleanup_remaining=0\npool_closed=true\n", StandardOpenOption.CREATE, StandardOpenOption.APPEND);
    }
    private static void cleanupAndClose(ConfigurableApplicationContext target, Runnable cleanup) {
        try { cleanup.run(); } finally { target.close(); }
    }
    @Test void cleanupFailureStillClosesRealContextAndPool() {
        var isolated = new GenericApplicationContext();
        isolated.registerBean(HikariDataSource.class);
        isolated.refresh();
        var isolatedPool = isolated.getBean(HikariDataSource.class);
        assertThrows(IllegalStateException.class, () -> cleanupAndClose(isolated, () -> { throw new IllegalStateException("injected cleanup failure"); }));
        assertFalse(isolated.isActive()); assertTrue(isolatedPool.isClosed());
    }
    @Test void testDatabasePropertiesOverrideInvalidDefaults() {
        var defaults = context.getEnvironment().getPropertySources().get("defaultProperties");
        assertNotNull(defaults);
        assertEquals("jdbc:postgresql://127.0.0.1:1/invalid", defaults.getProperty("spring.datasource.url"));
        assertEquals(scopedUrl(), context.getEnvironment().getProperty("spring.datasource.url"));
        assertEquals(scopedUrl(), pool.getJdbcUrl()); assertEquals(4, pool.getMaximumPoolSize());
    }
    private TaskAcceptanceStore.Request request(UUID conversation, UUID operationId, String operation, Instant time, String digest) {
        return new TaskAcceptanceStore.Request("principal", operationId, operation, time, digest, conversation, 0);
    }
    private long count(String table, UUID conversation) {
        return jdbc.sql("SELECT COUNT(*) FROM " + table + " WHERE conversation_id=:id").param("id", conversation.toString()).query(Long.class).single();
    }
    @Test void persistedReceiptReadback() {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
        var request = request(conversation, UUID.randomUUID(), "accept", now, "digest");
        var receipt = store.acceptTask(request);
        assertEquals(receipt, store.acceptTask(request));
        var snapshot = store.snapshot(conversation);
        assertEquals(receipt.taskId(), snapshot.activeTaskId()); assertEquals("QUEUED", snapshot.activeState());
        assertEquals(1, snapshot.revision()); assertEquals(1, snapshot.sequence());
        assertEquals(1, count("task_run", conversation)); assertEquals(1, count("journal_event", conversation)); assertEquals(1, count("request_receipt", conversation));
    }
    @Test void duplicateRequestsConcurrentlyReturnOneReceipt() throws Exception {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
        var request = request(conversation, UUID.randomUUID(), "accept", now, "digest");
        var latch = new CountDownLatch(1);
        try (var executor = Executors.newFixedThreadPool(2)) {
            var a = executor.submit(() -> { latch.await(); return store.acceptTask(request); });
            var b = executor.submit(() -> { latch.await(); return store.acceptTask(request); });
            latch.countDown(); assertEquals(a.get(10, TimeUnit.SECONDS), b.get(10, TimeUnit.SECONDS));
        }
        assertEquals(1, count("task_run", conversation)); assertEquals(1, count("request_receipt", conversation)); assertEquals(1, count("journal_event", conversation));
    }
    @Test void identityChangesConflict() {
        UUID conversation = UUID.randomUUID(); UUID op = UUID.randomUUID(); store.createConversation(conversation);
        store.acceptTask(request(conversation, op, "accept", now, "digest"));
        for (var changed : List.of(request(conversation, op, "other", now, "digest"), request(conversation, op, "accept", now, "other"), request(conversation, op, "accept", now.minusSeconds(1), "digest")))
            assertThrows(TaskAcceptanceStore.Conflict.class, () -> store.acceptTask(changed));
        var revised = new TaskAcceptanceStore.Request("principal", op, "accept", now, "digest", conversation, 1);
        assertThrows(TaskAcceptanceStore.Conflict.class, () -> store.acceptTask(revised));
        assertEquals(1, count("task_run", conversation)); assertEquals(1, count("request_receipt", conversation));
    }
    @Test void competingIdsRollbackLoser() throws Exception {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation); var latch = new CountDownLatch(1);
        try (var executor = Executors.newFixedThreadPool(2)) {
            var futures = List.of(executor.submit(() -> compete(conversation, latch)), executor.submit(() -> compete(conversation, latch)));
            latch.countDown(); int wins = 0; for (var f : futures) if (f.get(10, TimeUnit.SECONDS)) wins++;
            assertEquals(1, wins);
        }
        assertEquals(1, count("task_run", conversation)); assertEquals(1, count("request_receipt", conversation)); assertEquals(1, count("journal_event", conversation));
    }
    private boolean compete(UUID conversation, CountDownLatch latch) throws InterruptedException {
        latch.await(); try { store.acceptTask(request(conversation, UUID.randomUUID(), "accept", now, "digest")); return true; }
        catch (TaskAcceptanceStore.Conflict expected) { return false; }
    }
    @Test void ambientTransactionIsRejectedBeforeWrites() {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
        assertThrows(IllegalStateException.class, () -> tx.execute(status -> store.acceptTask(request(conversation, UUID.randomUUID(), "accept", now, "digest"))));
        assertEquals(0, count("task_run", conversation)); assertEquals(0, count("request_receipt", conversation)); assertEquals(0, count("journal_event", conversation));
        var snapshot = store.snapshot(conversation); assertEquals(0, snapshot.revision()); assertEquals(0, snapshot.sequence()); assertNull(snapshot.activeTaskId());
    }
    @Test void databaseConstraintFailureRollsBackAllAcceptanceWrites() {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
        jdbc.sql("ALTER TABLE journal_event ADD CONSTRAINT test_failure CHECK (conversation_id <> '" + conversation + "' OR sequence < 0)").update();
        try {
            assertThrows(org.springframework.dao.DataIntegrityViolationException.class,
                    () -> store.acceptTask(request(conversation, UUID.randomUUID(), "accept", now, "digest")));
            assertEquals(0, count("task_run", conversation)); assertEquals(0, count("request_receipt", conversation)); assertEquals(0, count("journal_event", conversation));
            var snapshot = store.snapshot(conversation); assertEquals(0, snapshot.revision()); assertEquals(0, snapshot.sequence()); assertNull(snapshot.activeTaskId());
        } finally { jdbc.sql("ALTER TABLE journal_event DROP CONSTRAINT test_failure").update(); }
    }
    @Test void invalidTimeAndRetentionAreRejected() {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
        assertThrows(IllegalArgumentException.class, () -> store.acceptTask(request(conversation, UUID.randomUUID(), "accept", now.minusSeconds(3601), "digest")));
        assertThrows(IllegalArgumentException.class, () -> store.acceptTask(request(conversation, UUID.randomUUID(), "accept", now.plusSeconds(61), "digest")));
        assertThrows(IllegalArgumentException.class, () -> new TaskAcceptanceStore.Retention(Duration.ofHours(1), Duration.ZERO, Duration.ofMinutes(1), Duration.ofHours(1)));
        assertEquals(0, count("task_run", conversation));
    }
    @Test void instantSurvivesDifferentJvmTimezones() {
        TimeZone original = TimeZone.getDefault();
        var clock = (MutableClock) context.getBean(Clock.class);
        try {
            for (String zone : List.of("UTC", "Asia/Seoul", "America/New_York")) {
                TimeZone.setDefault(TimeZone.getTimeZone(zone));
                for (Instant instant : List.of(Instant.parse("2026-03-08T02:30:00.123Z"), Instant.parse("2026-11-01T01:30:00.123Z"), Instant.parse("2026-03-08T00:00:00.123Z"))) {
                    clock.set(instant);
                    UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
                    var request = request(conversation, UUID.randomUUID(), "accept", instant, "digest");
                    var receipt = store.acceptTask(request);
                    LocalDateTime stored = jdbc.sql("SELECT created_at FROM request_receipt WHERE operation_id=:id").param("id", request.operationId().toString())
                            .query((rs, row) -> rs.getObject(1, LocalDateTime.class)).single();
                    assertEquals(instant, stored.toInstant(ZoneOffset.UTC)); assertEquals(receipt, store.acceptTask(request));
                }
            }
        } finally { clock.set(now); TimeZone.setDefault(original); }
    }
    @Test void concurrentSnapshotAlwaysPairsStateWithWatermark() throws Exception {
        UUID conversation = UUID.randomUUID(); store.createConversation(conversation);
        var written = new CountDownLatch(1); var allowCommit = new CountDownLatch(1);
        try (var executor = Executors.newSingleThreadExecutor()) {
            var taskId = UUID.randomUUID();
            var writer = executor.submit(() -> tx.execute(status -> {
                jdbc.sql("INSERT INTO task_run(id,row_version,conversation_id,state) VALUES (:id,0,:conversation,'QUEUED')").param("id", taskId.toString()).param("conversation", conversation.toString()).update();
                jdbc.sql("UPDATE conversation SET revision=1,durable_sequence=1,active_task=:id WHERE id=:conversation").param("id", taskId.toString()).param("conversation", conversation.toString()).update();
                jdbc.sql("INSERT INTO journal_event(id,row_version,conversation_id,sequence,task_id) VALUES (:event,0,:conversation,1,:id)").param("event", UUID.randomUUID().toString()).param("id", taskId.toString()).param("conversation", conversation.toString()).update();
                written.countDown();
                try { if (!allowCommit.await(10, TimeUnit.SECONDS)) throw new IllegalStateException("reader did not finish"); }
                catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); throw new IllegalStateException(interrupted); }
                return taskId;
            }));
            try {
                assertTrue(written.await(10, TimeUnit.SECONDS));
                var before = store.snapshot(conversation);
                assertEquals(0, before.revision()); assertEquals(0, before.sequence()); assertNull(before.activeTaskId()); assertNull(before.activeState());
            } finally { allowCommit.countDown(); }
            var committedTask = writer.get(10, TimeUnit.SECONDS);
            var after = store.snapshot(conversation);
            assertEquals(1, after.revision()); assertEquals(1, after.sequence()); assertEquals(committedTask, after.activeTaskId()); assertEquals("QUEUED", after.activeState());
        }
    }
}
