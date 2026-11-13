package io.arcvera.core.bootstrap;

import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

@Component
public final class CredentialCleanup {
    private final CredentialRegistry registry;
    public CredentialCleanup(CredentialRegistry registry) { this.registry = registry; }

    @Scheduled(fixedDelay = 1000, initialDelay = 1000, scheduler = "bootstrapTaskScheduler")
    public void purgeExpired() { registry.purgeExpired(); }
}
