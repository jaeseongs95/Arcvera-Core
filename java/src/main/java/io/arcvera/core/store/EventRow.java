package io.arcvera.core.store;
import org.springframework.data.annotation.Id;
import org.springframework.data.annotation.Version;
import org.springframework.data.relational.core.mapping.Table;
import org.springframework.data.relational.core.mapping.Column;
@Table("journal_event")
record EventRow(@Id @Column("id") String id, @Version @Column("row_version") Long rowVersion, @Column("conversation_id") String conversationId, @Column("sequence") long sequence, @Column("task_id") String taskId) { }
