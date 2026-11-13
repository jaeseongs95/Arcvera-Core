package io.arcvera.core.store;
import org.springframework.data.annotation.Id;
import org.springframework.data.annotation.Version;
import org.springframework.data.relational.core.mapping.Table;
import org.springframework.data.relational.core.mapping.Column;
@Table("conversation")
record ConversationRow(@Id @Column("id") String id, @Version @Column("revision") Long revision, @Column("durable_sequence") long durableSequence, @Column("active_task") String activeTask) { }
