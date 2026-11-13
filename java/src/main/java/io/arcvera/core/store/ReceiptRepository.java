package io.arcvera.core.store;
import java.util.Optional;
import org.springframework.data.jdbc.repository.query.Query;
import org.springframework.data.repository.Repository;
import org.springframework.data.repository.query.Param;
interface ReceiptRepository extends Repository<ReceiptRow, String> {
    @Query(value = "SELECT r.id,r.row_version,i.principal,i.operation_id,i.operation,i.created_at,i.payload_digest,r.conversation_id,r.expected_revision,r.proof_lease,r.proof_revision,r.proof_client,r.task_id,r.revision,r.sequence FROM request_receipt r JOIN operation_identity i ON r.id=i.id WHERE i.principal = :principal AND i.operation_id = :operationId", rowMapperClass = ReceiptRowMapper.class)
    Optional<ReceiptRow> findReceipt(@Param("principal") String principal, @Param("operationId") String operationId);
}
