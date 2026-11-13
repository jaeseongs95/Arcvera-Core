package io.arcvera.core.store;
import java.time.LocalDateTime;
import org.springframework.data.annotation.Id;
import org.springframework.data.annotation.Version;
import org.springframework.data.relational.core.mapping.Table;
import org.springframework.data.relational.core.mapping.Column;
@Table("request_receipt")
record ReceiptRow(@Id @Column("id") String id, @Version @Column("row_version") Long rowVersion, @Column("principal") String principal,
                  @Column("operation_id") String operationId, @Column("operation") String operation, @Column("created_at") LocalDateTime createdAt,
                  @Column("payload_digest") String payloadDigest, @Column("conversation_id") String conversationId,
                  @Column("expected_revision") long expectedRevision, @Column("task_id") String taskId,
                  @Column("revision") Long revision, @Column("sequence") Long sequence) { }
