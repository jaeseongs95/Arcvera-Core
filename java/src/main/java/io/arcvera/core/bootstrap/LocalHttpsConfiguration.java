package io.arcvera.core.bootstrap;

import java.net.InetAddress;
import java.util.Collections;
import jakarta.servlet.http.HttpServletRequest;
import org.springframework.boot.ssl.DefaultSslBundleRegistry;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.boot.web.server.ConfigurableWebServerFactory;
import org.springframework.boot.web.server.Ssl;
import org.springframework.boot.web.server.WebServerFactoryCustomizer;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configuration.EnableWebSecurity;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.oauth2.server.resource.introspection.OpaqueTokenIntrospector;
import org.springframework.security.web.SecurityFilterChain;

/** Imported explicitly by the local server assembly, after its protection prerequisites are established. */
@Configuration(proxyBeanMethods = false)
@EnableWebSecurity
@ConditionalOnProperty(name = "arcvera.bootstrap.explicit-https", havingValue = "true")
public class LocalHttpsConfiguration {
    @Bean
    WebServerFactoryCustomizer<ConfigurableWebServerFactory> localHttpsServer(LocalTlsMaterial material) {
        return factory -> {
            try {
                factory.setAddress(InetAddress.getByAddress(new byte[]{127, 0, 0, 1}));
            } catch (java.net.UnknownHostException impossible) {
                throw new IllegalStateException("Invalid loopback bytes", impossible);
            }
            factory.setPort(0);
            var bundles = new DefaultSslBundleRegistry("arcvera-local", material.serverBundle());
            factory.setSslBundles(bundles);
            var ssl = Ssl.forBundle("arcvera-local");
            ssl.setEnabledProtocols(new String[]{"TLSv1.3", "TLSv1.2"});
            factory.setSsl(ssl);
        };
    }

    @Bean
    SecurityFilterChain localSecurity(HttpSecurity http, OpaqueTokenIntrospector introspector) throws Exception {
        http.authorizeHttpRequests(authorize -> authorize
                        .requestMatchers(LocalHttpsConfiguration::invalidAuthority).denyAll()
                        .anyRequest().authenticated())
                .sessionManagement(session -> session.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
                .requestCache(cache -> cache.disable())
                .httpBasic(basic -> basic.disable())
                .formLogin(form -> form.disable())
                .oauth2ResourceServer(oauth -> oauth.opaqueToken(opaque -> opaque.introspector(introspector)));
        return http.build();
    }

    private static boolean invalidAuthority(HttpServletRequest request) {
        var hosts = Collections.list(request.getHeaders("Host"));
        return hosts.size() != 1 || !("127.0.0.1:" + request.getLocalPort()).equals(hosts.getFirst())
                || !"127.0.0.1".equals(request.getServerName())
                || request.getServerPort() != request.getLocalPort() || request.getHeader("Origin") != null;
    }
}
