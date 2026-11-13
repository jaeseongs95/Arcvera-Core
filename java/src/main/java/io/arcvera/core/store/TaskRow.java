package io.arcvera.core.store;
import org.springframework.data.annotation.Id;
import org.springframework.data.annotation.Version;
import org.springframework.data.relational.core.mapping.Table;
import org.springframework.data.relational.core.mapping.Column;
@Table("task_run")
record TaskRow(@Id @Column("id") String id, @Version @Column("row_version") Long rowVersion, @Column("conversation_id") String conversationId, @Column("state") String state) { }
