package io.arcvera.core.bootstrap;

import java.io.IOException;
import java.security.GeneralSecurityException;
import java.util.Objects;
import java.util.UUID;
import org.springframework.boot.web.server.context.WebServerInitializedEvent;
import org.springframework.context.ApplicationContext;
import org.springframework.context.ApplicationContextException;
import org.springframework.context.ApplicationListener;
import org.springframework.context.SmartLifecycle;
import tools.jackson.databind.json.JsonMapper;

/** Explicitly assembled once per local application context; owns the supplied profile lock. */
public final class LocalBootstrapLifecycle implements SmartLifecycle, ApplicationListener<WebServerInitializedEvent> {
    private final ApplicationContext context;
    private final CredentialRegistry registry;
    private final WindowsProfileLock lock;
    private final JsonMapper mapper;
    private final UUID storeEpoch;
    private final LocalTlsMaterial tls;
    private BootstrapDescriptorPublisher publisher;
    private boolean running;
    private boolean closed;

    public LocalBootstrapLifecycle(ApplicationContext context, CredentialRegistry registry, WindowsProfileLock lock,
                                   JsonMapper mapper, UUID storeEpoch, LocalTlsMaterial tls) {
        this.context = Objects.requireNonNull(context);
        this.registry = Objects.requireNonNull(registry);
        this.lock = Objects.requireNonNull(lock);
        this.mapper = Objects.requireNonNull(mapper);
        this.storeEpoch = Objects.requireNonNull(storeEpoch);
        this.tls = Objects.requireNonNull(tls);
    }

    @Override public synchronized void onApplicationEvent(WebServerInitializedEvent event) {
        if (event.getApplicationContext() != context) return;
        if (closed || publisher != null) throw new ApplicationContextException("Duplicate local server initialization");
        try {
            publisher = new BootstrapDescriptorPublisher(registry, lock, mapper, storeEpoch, event.getWebServer().getPort(), tls);
            publisher.issue();
        } catch (IOException | GeneralSecurityException | RuntimeException failure) {
            registry.revoke();
            var startupFailure = new ApplicationContextException("Local bootstrap publication failed", failure);
            try { stop(); } catch (RuntimeException | Error cleanupFailure) { startupFailure.addSuppressed(cleanupFailure); }
            throw startupFailure;
        } catch (Error failure) {
            registry.revoke();
            try { stop(); } catch (RuntimeException | Error cleanupFailure) { failure.addSuppressed(cleanupFailure); }
            throw failure;
        }
    }

    public synchronized BootstrapDescriptorPublisher publisher() {
        if (closed || publisher == null) throw new IllegalStateException("Local bootstrap unavailable");
        return publisher;
    }

    @Override public synchronized void start() {
        if (closed) throw new IllegalStateException("Local bootstrap closed");
        running = true;
    }

    @Override public synchronized void stop() {
        if (closed) return;
        closed = true;
        running = false;
        registry.revoke();
        IOException failure = null;
        try { if (publisher != null) publisher.close(); }
        catch (IOException e) { failure = e; }
        try { lock.close(); }
        catch (IOException e) { if (failure == null) failure = e; else failure.addSuppressed(e); }
        if (failure != null) throw new IllegalStateException("Local bootstrap cleanup failed", failure);
    }

    @Override public synchronized boolean isRunning() { return running; }
    @Override public int getPhase() { return Integer.MAX_VALUE; }
}
