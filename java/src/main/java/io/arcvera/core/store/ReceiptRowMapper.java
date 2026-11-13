package io.arcvera.core.store;

import java.sql.ResultSet;
import java.sql.SQLException;
import java.time.LocalDateTime;
import org.springframework.jdbc.core.RowMapper;

/** Read ANSI timestamp as UTC calendar fields without a JVM-zone Timestamp conversion. */
public final class ReceiptRowMapper implements RowMapper<ReceiptRow> {
    @Override public ReceiptRow mapRow(ResultSet row, int rowNumber) throws SQLException {
        return new ReceiptRow(row.getString("id"), row.getObject("row_version", Long.class), row.getString("principal"),
                row.getString("operation_id"), row.getString("operation"), row.getObject("created_at", LocalDateTime.class),
                row.getString("payload_digest"), row.getString("conversation_id"), row.getLong("expected_revision"),
                row.getString("task_id"), row.getObject("revision", Long.class), row.getObject("sequence", Long.class));
    }
}
