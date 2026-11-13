package io.arcvera.core.store;

import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Service;
import org.springframework.transaction.support.TransactionSynchronizationManager;

/** Global request identity. Reserve and complete a typed receipt in one transaction. */
@Service
public final class OperationIdentityService {
    public record Identity(UUID id, String operation, Instant createdAt, String digest) { }
    private final JdbcClient jdbc;
    public OperationIdentityService(JdbcClient jdbc) { this.jdbc = jdbc; }
    public Optional<Identity> find(String principal, UUID operationId) {
        return jdbc.sql("SELECT id,operation,created_at,payload_digest FROM operation_identity WHERE principal=:principal AND operation_id=:operationId")
                .param("principal", principal).param("operationId", operationId.toString())
                .query((rs, n) -> new Identity(UUID.fromString(rs.getString(1)), rs.getString(2),
                        rs.getObject(3, LocalDateTime.class).toInstant(ZoneOffset.UTC), rs.getString(4))).optional();
    }
    public UUID reserve(String principal, UUID operationId, String operation, Instant createdAt, String digest) {
        if (!TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("transaction required");
        UUID id = UUID.randomUUID();
        jdbc.sql("INSERT INTO operation_identity(id,principal,operation_id,operation,created_at,payload_digest) VALUES (:id,:principal,:operationId,:operation,:createdAt,:digest)")
                .param("id", id.toString()).param("principal", principal).param("operationId", operationId.toString())
                .param("operation", operation).param("createdAt", LocalDateTime.ofInstant(createdAt, ZoneOffset.UTC)).param("digest", digest).update();
        return id;
    }
    public void discard(UUID id) {
        if (!TransactionSynchronizationManager.isActualTransactionActive()) throw new IllegalStateException("transaction required");
        if (jdbc.sql("DELETE FROM operation_identity WHERE id=:id").param("id", id.toString()).update() != 1)
            throw new IllegalStateException("reservation missing");
    }
}
