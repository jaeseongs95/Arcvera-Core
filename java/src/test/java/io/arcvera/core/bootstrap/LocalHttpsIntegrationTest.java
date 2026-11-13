package io.arcvera.core.bootstrap;

import static org.junit.jupiter.api.Assertions.*;

import java.net.ProxySelector;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.security.KeyStore;
import java.security.SecureRandom;
import java.security.cert.X509Certificate;
import java.nio.file.Files;
import java.nio.file.Path;
import java.io.ByteArrayInputStream;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.security.cert.CertificateFactory;
import java.util.Base64;
import java.time.Clock;
import java.time.Duration;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;
import javax.net.ssl.SSLContext;
import javax.net.ssl.TrustManagerFactory;
import org.junit.jupiter.api.Test;
import org.springframework.boot.SpringBootConfiguration;
import org.springframework.boot.autoconfigure.EnableAutoConfiguration;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.boot.jdbc.autoconfigure.DataSourceAutoConfiguration;
import org.springframework.boot.web.server.context.WebServerApplicationContext;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.context.ApplicationContext;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;
import tools.jackson.databind.json.JsonMapper;

class LocalHttpsIntegrationTest {
    @SpringBootConfiguration(proxyBeanMethods = false)
    @EnableAutoConfiguration(exclude = DataSourceAutoConfiguration.class)
    @Import({LocalHttpsConfiguration.class, LocalCredentialIntrospector.class, Probe.class})
    static class FixtureApplication {
        @Bean Clock clock() { return Clock.systemUTC(); }
        @Bean SecureRandom random() { return new SecureRandom(); }
        @Bean AtomicLong ticker() { return new AtomicLong(); }
        @Bean CredentialRegistry registry(Clock clock, SecureRandom random, AtomicLong ticker, WindowsProfileLock lock) {
            return new CredentialRegistry(clock, random, ticker::get, UUID.randomUUID(), lock.sid(), Set.of("read"));
        }
        @Bean LocalTlsMaterial localTlsMaterial(Clock clock, SecureRandom random) throws Exception {
            return new LocalTlsMaterial(clock, random, "127.0.0.1");
        }
        @Bean JsonMapper mapper() { return JsonMapper.builder().build(); }
        @Bean LocalBootstrapLifecycle localBootstrapLifecycle(ApplicationContext context, CredentialRegistry registry,
                WindowsProfileLock lock, JsonMapper mapper, LocalTlsMaterial tls) {
            return new LocalBootstrapLifecycle(context, registry, lock, mapper, UUID.randomUUID(), tls);
        }
    }

    @SpringBootConfiguration(proxyBeanMethods = false)
    @EnableAutoConfiguration(exclude = DataSourceAutoConfiguration.class)
    @Import({LocalHttpsConfiguration.class, LocalCredentialIntrospector.class, Probe.class})
    static class RandomFailureApplication {
        @Bean JsonMapper mapper() { return JsonMapper.builder().build(); }
        @Bean LocalBootstrapLifecycle localBootstrapLifecycle(ApplicationContext context, CredentialRegistry registry,
                WindowsProfileLock lock, JsonMapper mapper, LocalTlsMaterial tls) {
            return new LocalBootstrapLifecycle(context, registry, lock, mapper, UUID.randomUUID(), tls);
        }
    }

    @RestController
    static class Probe {
        @GetMapping("/probe") String probe() { return "ok"; }
    }

    @Test void realHttpsBearerRotationAndRevocation() throws Exception {
        Path home = Files.createTempDirectory(Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap"), "https-fixture-");
        UUID profile = UUID.randomUUID();
        Path directory = home.resolve("runtime/core2").resolve(profile.toString());
        Path descriptor = directory.resolve("connection.json");
        var savedRegistry = new AtomicReference<CredentialRegistry>();
        var savedToken = new AtomicReference<String>();
        try (var lock = new WindowsProfileLock(home, profile);
             var context = new SpringApplicationBuilder(FixtureApplication.class)
                .properties("arcvera.bootstrap.explicit-https=true", "spring.main.banner-mode=off")
                .initializers(application -> application.getBeanFactory().registerSingleton("windowsProfileLock", lock))
                .run()) {
            var port = ((WebServerApplicationContext) context).getWebServer().getPort();
            var registry = context.getBean(CredentialRegistry.class);
            savedRegistry.set(registry);
            var tls = context.getBean(LocalTlsMaterial.class);
            var mapper = context.getBean(JsonMapper.class);
            var publisher = context.getBean(LocalBootstrapLifecycle.class).publisher();
            var document = mapper.readTree(Files.readAllBytes(descriptor));
            String oldToken = document.get("credential").asString();
            savedToken.set(oldToken);
            UUID oldId = UUID.fromString(document.get("credentialId").asString());
            long oldGeneration = Long.parseLong(document.get("generation").asString());
            var endpointData = document.get("endpoint");
            assertEquals(tls.spkiSha256(), endpointData.get("serverSpkiSha256").asString());
            var anchor = (X509Certificate) CertificateFactory.getInstance("X.509").generateCertificate(
                    new ByteArrayInputStream(Base64.getDecoder().decode(endpointData.get("trustAnchorDer").asString())));
            URI endpoint = URI.create("https://127.0.0.1:" + port + "/probe");
            try (HttpClient client = client(anchor)) {
                assertEquals(401, send(client, endpoint, null));
                assertEquals(401, send(client, endpoint, "invalid"));
                assertEquals(200, send(client, endpoint, document.get("credential").asString()));
                var foreignOrigin = HttpRequest.newBuilder(endpoint).header("Authorization", "Bearer " + oldToken)
                        .header("Origin", "https://example.invalid").GET().build();
                assertEquals(403, client.send(foreignOrigin, HttpResponse.BodyHandlers.discarding()).statusCode());
                assertEquals(403, sendWrongHost(anchor, port, oldToken));
                context.getBean(AtomicLong.class).set(Duration.ofMinutes(13).toNanos());
                var next = publisher.rotate(oldId, oldGeneration);
                var rotated = mapper.readTree(Files.readAllBytes(descriptor));
                assertEquals(200, send(client, endpoint, oldToken));
                assertEquals(200, send(client, endpoint, rotated.get("credential").asString()));
                context.getBean(AtomicLong.class).addAndGet(Duration.ofSeconds(30).toNanos());
                assertEquals(401, send(client, endpoint, oldToken));
                assertTrue(registry.authenticate(next.token()).isPresent());
                savedToken.set(next.token());
                registry.revoke();
                assertEquals(401, send(client, endpoint, next.token()));
            }
            var other = new LocalTlsMaterial(Clock.systemUTC(), new SecureRandom(), "127.0.0.1");
            try (HttpClient wrongTrust = client(other.certificate())) {
                assertThrows(javax.net.ssl.SSLHandshakeException.class, () -> send(wrongTrust, endpoint, oldToken));
            }
        } finally {
            if (savedRegistry.get() != null) {
                assertTrue(savedRegistry.get().authenticate(savedToken.get()).isEmpty());
                assertFalse(Files.exists(descriptor));
            }
            Files.deleteIfExists(descriptor);
            Files.deleteIfExists(directory.resolve("profile.lock"));
            Files.deleteIfExists(directory);
            Files.deleteIfExists(home.resolve("runtime/core2"));
            Files.deleteIfExists(home.resolve("runtime"));
            Files.deleteIfExists(home);
        }
    }

    private static HttpClient client(X509Certificate anchor) throws Exception {
        return HttpClient.newBuilder().sslContext(sslContext(anchor)).proxy(ProxySelector.of(null)).connectTimeout(Duration.ofSeconds(5)).build();
    }

    @Test void contextShutdownRevokesLiveCredentialAndWithdrawsDescriptor() throws Exception {
        Path home = Files.createTempDirectory(Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap"), "lifecycle-fixture-");
        UUID profile = UUID.randomUUID();
        Path directory = home.resolve("runtime/core2").resolve(profile.toString());
        Path descriptor = directory.resolve("connection.json");
        try (var lock = new WindowsProfileLock(home, profile)) {
            CredentialRegistry registry;
            String token;
            try (var context = new SpringApplicationBuilder(FixtureApplication.class)
                    .properties("arcvera.bootstrap.explicit-https=true", "spring.main.banner-mode=off")
                    .initializers(application -> application.getBeanFactory().registerSingleton("windowsProfileLock", lock))
                    .run()) {
                registry = context.getBean(CredentialRegistry.class);
                token = context.getBean(JsonMapper.class).readTree(Files.readAllBytes(descriptor)).get("credential").asString();
                assertTrue(registry.authenticate(token).isPresent());
            }
            assertTrue(registry.authenticate(token).isEmpty());
            assertFalse(Files.exists(descriptor));
            try (var reacquired = new WindowsProfileLock(home, profile)) {
                assertEquals(directory, reacquired.directory());
            }
        } finally {
            Files.deleteIfExists(descriptor);
            Files.deleteIfExists(directory.resolve("profile.lock"));
            Files.deleteIfExists(directory);
            Files.deleteIfExists(home.resolve("runtime/core2"));
            Files.deleteIfExists(home.resolve("runtime"));
            Files.deleteIfExists(home);
        }
    }

    @Test void startupPublicationConflictReleasesLockWithoutDeletingForeignFile() throws Exception {
        Path home = Files.createTempDirectory(Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap"), "startup-failure-fixture-");
        UUID profile = UUID.randomUUID();
        Path directory = home.resolve("runtime/core2").resolve(profile.toString());
        Path descriptor = directory.resolve("connection.json");
        try (var lock = new WindowsProfileLock(home, profile)) {
            Files.writeString(descriptor, "foreign");
            assertThrows(RuntimeException.class, () -> new SpringApplicationBuilder(FixtureApplication.class)
                    .properties("arcvera.bootstrap.explicit-https=true", "spring.main.banner-mode=off")
                    .initializers(application -> application.getBeanFactory().registerSingleton("windowsProfileLock", lock))
                    .run());
            assertEquals("foreign", Files.readString(descriptor));
            try (var reacquired = new WindowsProfileLock(home, profile)) {
                assertEquals(directory, reacquired.directory());
            }
        } finally {
            Files.deleteIfExists(descriptor);
            Files.deleteIfExists(directory.resolve("profile.lock"));
            Files.deleteIfExists(directory);
            Files.deleteIfExists(home.resolve("runtime/core2"));
            Files.deleteIfExists(home.resolve("runtime"));
            Files.deleteIfExists(home);
        }
    }

    @Test void startupRandomErrorRevokesAndReleasesLockAndPreservesForeignFile() throws Exception {
        Path home = Files.createTempDirectory(Path.of("D:/codex/build/core200-java25-20261113/reports/bootstrap"), "random-error-fixture-");
        UUID profile = UUID.randomUUID();
        Path directory = home.resolve("runtime/core2").resolve(profile.toString());
        Path descriptor = directory.resolve("connection.json");
        var expected = new AssertionError("fixture random failure");
        try (var lock = new WindowsProfileLock(home, profile)) {
            Files.writeString(descriptor, "foreign");
            var clock = Clock.systemUTC();
            var registry = new CredentialRegistry(clock, new FailingRandom(expected), System::nanoTime,
                    UUID.randomUUID(), lock.sid(), Set.of("read"));
            var tls = new LocalTlsMaterial(clock, new SecureRandom(), "127.0.0.1");
            Throwable failure = assertThrows(Throwable.class, () -> {
                try (var context = new SpringApplicationBuilder(RandomFailureApplication.class)
                        .properties("arcvera.bootstrap.explicit-https=true", "spring.main.banner-mode=off")
                        .initializers(application -> {
                            application.getBeanFactory().registerSingleton("windowsProfileLock", lock);
                            application.getBeanFactory().registerSingleton("credentialRegistry", registry);
                            application.getBeanFactory().registerSingleton("localTlsMaterial", tls);
                        }).run()) {
                    assertFalse(context.isActive());
                }
            });
            boolean found = false;
            for (Throwable cause = failure; cause != null; cause = cause.getCause()) {
                if (cause == expected) found = true;
            }
            assertTrue(found, "original Error must remain in the cause chain");
            assertThrows(IllegalStateException.class, registry::issue);
            assertEquals("foreign", Files.readString(descriptor));
            try (var reacquired = new WindowsProfileLock(home, profile)) {
                assertEquals(directory, reacquired.directory());
            }
        } finally {
            Files.deleteIfExists(descriptor);
            Files.deleteIfExists(directory.resolve("profile.lock"));
            Files.deleteIfExists(directory);
            Files.deleteIfExists(home.resolve("runtime/core2"));
            Files.deleteIfExists(home.resolve("runtime"));
            Files.deleteIfExists(home);
        }
    }

    private static final class FailingRandom extends SecureRandom {
        private static final long serialVersionUID = 1L;
        private final Error failure;
        FailingRandom(Error failure) { this.failure = failure; }
        @Override public void nextBytes(byte[] bytes) { throw failure; }
    }

    private static SSLContext sslContext(X509Certificate anchor) throws Exception {
        var trustStore = KeyStore.getInstance("PKCS12");
        trustStore.load(null, null);
        trustStore.setCertificateEntry("local", anchor);
        var managers = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm());
        managers.init(trustStore);
        var ssl = SSLContext.getInstance("TLS");
        ssl.init(null, managers.getTrustManagers(), null);
        return ssl;
    }

    private static int sendWrongHost(X509Certificate anchor, int port, String token) throws Exception {
        try (var socket = (javax.net.ssl.SSLSocket) sslContext(anchor).getSocketFactory().createSocket("127.0.0.1", port)) {
            var params = socket.getSSLParameters();
            params.setEndpointIdentificationAlgorithm("HTTPS");
            socket.setSSLParameters(params);
            socket.setSoTimeout(5000);
            socket.startHandshake();
            var output = socket.getOutputStream();
            output.write(("GET /probe HTTP/1.1\r\nHost: example.invalid:" + port
                    + "\r\nAuthorization: Bearer " + token + "\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
            output.flush();
            var line = new BufferedReader(new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII)).readLine();
            return Integer.parseInt(line.substring(9, 12));
        }
    }

    private static int send(HttpClient client, URI endpoint, String token) throws Exception {
        var request = HttpRequest.newBuilder(endpoint).timeout(Duration.ofSeconds(5)).GET();
        if (token != null) request.header("Authorization", "Bearer " + token);
        return client.send(request.build(), HttpResponse.BodyHandlers.discarding()).statusCode();
    }
}
