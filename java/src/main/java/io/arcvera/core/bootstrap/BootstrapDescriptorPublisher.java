package io.arcvera.core.bootstrap;

import java.io.IOException;
import java.security.GeneralSecurityException;
import java.time.format.DateTimeFormatter;
import java.time.format.DateTimeFormatterBuilder;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import java.time.Instant;
import tools.jackson.databind.json.JsonMapper;

/** Couples registry transition to protected publication while its profile lock is held. */
public final class BootstrapDescriptorPublisher implements AutoCloseable {
    private static final DateTimeFormatter TIMESTAMP = new DateTimeFormatterBuilder().appendInstant(3).toFormatter();
    private final CredentialRegistry registry;
    private final WindowsProfileLock lock;
    private final JsonMapper mapper;
    private final UUID storeEpoch;
    private final int port;
    private final String anchor;
    private final String pin;
    private final Instant certificateExpiry;

    public BootstrapDescriptorPublisher(CredentialRegistry registry, WindowsProfileLock lock, JsonMapper mapper,
                                        UUID storeEpoch, int port, LocalTlsMaterial tls) throws GeneralSecurityException {
        this.registry = Objects.requireNonNull(registry);
        this.lock = Objects.requireNonNull(lock);
        this.mapper = Objects.requireNonNull(mapper);
        this.storeEpoch = Objects.requireNonNull(storeEpoch);
        if (port < 1 || port > 65535) throw new IllegalArgumentException("Invalid loopback port");
        if (!lock.sid().equals(registry.binding().osPrincipal())) throw new IllegalArgumentException("OS principal mismatch");
        if (registry.binding().scopes().size() > 128) throw new IllegalArgumentException("Too many scopes");
        this.port = port;
        if (!"127.0.0.1".equals(Objects.requireNonNull(tls).numericLoopback())) throw new IllegalArgumentException("TLS address mismatch");
        this.anchor = tls.trustAnchorDer();
        this.pin = tls.spkiSha256();
        this.certificateExpiry = tls.certificate().getNotAfter().toInstant();
        registry.bindCertificateExpiry(certificateExpiry);
    }

    public CredentialRegistry.IssuedCredential issue() throws IOException { return registry.issueAndPublish(this::publish); }
    public CredentialRegistry.IssuedCredential rotate(UUID id, long generation) throws IOException {
        return registry.rotateAndPublish(id, generation, this::publish);
    }

    @Override public void close() throws IOException {
        registry.revoke();
        lock.withdraw();
    }

    private void publish(CredentialRegistry.IssuedCredential credential) throws IOException {
        if (credential.expiresAt().isAfter(certificateExpiry)) {
            throw new CredentialRegistry.PublicationNotCommittedException("TLS certificate expires before credential", null);
        }
        var binding = registry.binding();
        var descriptor = new Descriptor(1, "1.0", UUID.randomUUID().toString(), binding.instanceId().toString(),
                storeEpoch.toString(), lock.profileId().toString(), credential.credentialId().toString(),
                Long.toString(credential.generation()), new Principal(binding.osPrincipal(), binding.scopes().stream().sorted().toList()),
                lock.sid(), new Endpoint("https", "127.0.0.1", port, anchor, pin), credential.token(),
                TIMESTAMP.format(credential.issuedAt()), TIMESTAMP.format(credential.expiresAt()));
        byte[] bytes = mapper.writeValueAsBytes(descriptor);
        try { lock.publish(bytes); }
        finally { Arrays.fill(bytes, (byte) 0); }
    }

    private record Principal(String id, List<String> scopes) { }
    private record Endpoint(String scheme, String host, int port, String trustAnchorDer, String serverSpkiSha256) { }
    private record Descriptor(int schemaMajor, String protocolVersion, String descriptorId, String instanceId,
                              String storeEpoch, String profileId, String credentialId, String generation,
                              Principal principal, String osUserSid, Endpoint endpoint, String credential,
                              String issuedAt, String expiresAt) {
        @Override public String toString() { return "Descriptor[redacted]"; }
    }
}
