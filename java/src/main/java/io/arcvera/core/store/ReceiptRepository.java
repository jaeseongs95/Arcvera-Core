package io.arcvera.core.store;
import java.util.Optional;
import org.springframework.data.jdbc.repository.query.Query;
import org.springframework.data.repository.CrudRepository;
import org.springframework.data.repository.query.Param;
interface ReceiptRepository extends CrudRepository<ReceiptRow, String> {
    @Query(value = "SELECT * FROM request_receipt WHERE principal = :principal AND operation_id = :operationId", rowMapperClass = ReceiptRowMapper.class)
    Optional<ReceiptRow> findReceipt(@Param("principal") String principal, @Param("operationId") String operationId);
}
