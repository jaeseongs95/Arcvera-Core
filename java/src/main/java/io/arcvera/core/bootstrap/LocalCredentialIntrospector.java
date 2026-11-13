package io.arcvera.core.bootstrap;

import java.util.Map;
import org.springframework.security.core.GrantedAuthority;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.oauth2.core.DefaultOAuth2AuthenticatedPrincipal;
import org.springframework.security.oauth2.core.OAuth2AuthenticatedPrincipal;
import org.springframework.security.oauth2.server.resource.introspection.BadOpaqueTokenException;
import org.springframework.security.oauth2.server.resource.introspection.OpaqueTokenIntrospector;
import org.springframework.stereotype.Component;

@Component
public final class LocalCredentialIntrospector implements OpaqueTokenIntrospector {
    private final CredentialRegistry registry;

    public LocalCredentialIntrospector(CredentialRegistry registry) { this.registry = registry; }

    @Override
    public OAuth2AuthenticatedPrincipal introspect(String token) {
        var claims = registry.authenticate(token).orElseThrow(() -> new BadOpaqueTokenException("Invalid local credential"));
        var authorities = claims.scopes().stream().<GrantedAuthority>map(scope -> new SimpleGrantedAuthority("SCOPE_" + scope)).toList();
        return new DefaultOAuth2AuthenticatedPrincipal(claims.osPrincipal(), Map.of(
                "instanceId", claims.instanceId().toString(),
                "credentialId", claims.credentialId().toString(),
                "generation", claims.generation(),
                "scope", claims.scopes()), authorities);
    }
}
