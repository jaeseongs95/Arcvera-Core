package io.arcvera.core.bootstrap;

import static org.junit.jupiter.api.Assertions.*;

import java.io.ByteArrayInputStream;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;
import java.time.Clock;
import java.util.Base64;
import java.util.HexFormat;
import org.junit.jupiter.api.Test;

class LocalTlsMaterialTest {
    @Test void generatedCertificateIsSelfSignedAndHasRequiredServerExtensions() throws Exception {
        var material = new LocalTlsMaterial(Clock.systemUTC(), new SecureRandom(), "127.0.0.1");
        var certificate = material.certificate();
        certificate.checkValidity();
        certificate.verify(certificate.getPublicKey());
        assertEquals(certificate.getSubjectX500Principal(), certificate.getIssuerX500Principal());
        assertEquals(-1, certificate.getBasicConstraints());
        assertTrue(certificate.getKeyUsage()[0]);
        assertTrue(certificate.getExtendedKeyUsage().contains("1.3.6.1.5.5.7.3.1"));
        assertTrue(certificate.getSubjectAlternativeNames().stream()
                .anyMatch(name -> name.get(0).equals(7) && name.get(1).equals("127.0.0.1")));
        var encoded = Base64.getDecoder().decode(material.trustAnchorDer());
        var parsed = (X509Certificate) CertificateFactory.getInstance("X.509").generateCertificate(new ByteArrayInputStream(encoded));
        assertEquals(certificate, parsed);
        assertEquals("sha256:" + HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256")
                .digest(parsed.getPublicKey().getEncoded())), material.spkiSha256());
        assertNotNull(material.serverBundle().createSslContext());
        assertFalse(material.toString().contains(material.trustAnchorDer()));
    }

    @Test void nonNumericOrNonLoopbackHostIsRejected() {
        assertThrows(IllegalArgumentException.class, () -> new LocalTlsMaterial(Clock.systemUTC(), new SecureRandom(), "localhost"));
        assertThrows(IllegalArgumentException.class, () -> new LocalTlsMaterial(Clock.systemUTC(), new SecureRandom(), "0.0.0.0"));
    }
}
