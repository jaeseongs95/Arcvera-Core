package io.arcvera.core.bootstrap;

import java.math.BigInteger;
import java.security.GeneralSecurityException;
import java.security.KeyPairGenerator;
import java.security.KeyStore;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.security.cert.X509Certificate;
import java.security.spec.ECGenParameterSpec;
import java.time.Clock;
import java.time.Duration;
import java.util.Base64;
import java.util.Date;
import java.util.HexFormat;
import java.util.Objects;
import org.bouncycastle.asn1.x500.X500Name;
import org.bouncycastle.asn1.x509.BasicConstraints;
import org.bouncycastle.asn1.x509.ExtendedKeyUsage;
import org.bouncycastle.asn1.x509.Extension;
import org.bouncycastle.asn1.x509.GeneralName;
import org.bouncycastle.asn1.x509.GeneralNames;
import org.bouncycastle.asn1.x509.KeyPurposeId;
import org.bouncycastle.asn1.x509.KeyUsage;
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter;
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder;
import org.bouncycastle.operator.OperatorCreationException;
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder;
import org.springframework.boot.ssl.SslBundle;
import org.springframework.boot.ssl.SslBundleKey;
import org.springframework.boot.ssl.SslStoreBundle;

/** A process-local TLS identity. Its private key is never exported into the descriptor. */
public final class LocalTlsMaterial {
    private final X509Certificate certificate;
    private final SslBundle serverBundle;
    private final String numericLoopback;

    public LocalTlsMaterial(Clock clock, SecureRandom random, String numericLoopback)
            throws GeneralSecurityException, java.io.IOException, OperatorCreationException {
        Objects.requireNonNull(clock);
        Objects.requireNonNull(random);
        if (!"127.0.0.1".equals(numericLoopback) && !"::1".equals(numericLoopback)) {
            throw new IllegalArgumentException("Numeric loopback address required");
        }
        this.numericLoopback = numericLoopback;
        var generator = KeyPairGenerator.getInstance("EC");
        generator.initialize(new ECGenParameterSpec("secp256r1"), random);
        var keyPair = generator.generateKeyPair();
        var subject = new X500Name("CN=Arcvera local instance");
        var now = clock.instant();
        var builder = new JcaX509v3CertificateBuilder(subject, new BigInteger(128, random).setBit(127),
                Date.from(now.minusSeconds(30)), Date.from(now.plus(Duration.ofDays(1))), subject, keyPair.getPublic());
        builder.addExtension(Extension.basicConstraints, true, new BasicConstraints(false));
        builder.addExtension(Extension.keyUsage, true, new KeyUsage(KeyUsage.digitalSignature));
        builder.addExtension(Extension.extendedKeyUsage, false, new ExtendedKeyUsage(KeyPurposeId.id_kp_serverAuth));
        builder.addExtension(Extension.subjectAlternativeName, false,
                new GeneralNames(new GeneralName(GeneralName.iPAddress, numericLoopback)));
        var signer = new JcaContentSignerBuilder("SHA256withECDSA").setSecureRandom(random).build(keyPair.getPrivate());
        certificate = new JcaX509CertificateConverter().getCertificate(builder.build(signer));
        byte[] passwordBytes = new byte[32];
        random.nextBytes(passwordBytes);
        String password = Base64.getUrlEncoder().withoutPadding().encodeToString(passwordBytes);
        var store = KeyStore.getInstance("PKCS12");
        store.load(null, null);
        store.setKeyEntry("arcvera", keyPair.getPrivate(), password.toCharArray(), new java.security.cert.Certificate[]{certificate});
        serverBundle = SslBundle.of(SslStoreBundle.of(store, password, null), SslBundleKey.of(password, "arcvera"));
    }

    public X509Certificate certificate() { return certificate; }
    public String numericLoopback() { return numericLoopback; }
    public SslBundle serverBundle() { return serverBundle; }
    public String trustAnchorDer() throws GeneralSecurityException {
        return Base64.getEncoder().encodeToString(certificate.getEncoded());
    }
    public String spkiSha256() throws GeneralSecurityException {
        return "sha256:" + HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(certificate.getPublicKey().getEncoded()));
    }
    @Override public String toString() { return "LocalTlsMaterial[redacted]"; }
}
