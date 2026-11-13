package io.arcvera.core.bootstrap;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.scheduling.annotation.EnableScheduling;
import org.springframework.scheduling.concurrent.ThreadPoolTaskScheduler;

@Configuration(proxyBeanMethods = false)
@EnableScheduling
public class BootstrapSchedulingConfiguration {
    // A separate named scheduler bounds this registry's cleanup to one managed thread per context.
    @Bean(name = "bootstrapTaskScheduler", destroyMethod = "shutdown")
    ThreadPoolTaskScheduler bootstrapTaskScheduler() {
        var scheduler = new ThreadPoolTaskScheduler();
        scheduler.setPoolSize(1);
        scheduler.setThreadNamePrefix("bootstrap-cleanup-");
        scheduler.setRemoveOnCancelPolicy(true);
        return scheduler;
    }
}
