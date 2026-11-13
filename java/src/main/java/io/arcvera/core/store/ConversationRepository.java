package io.arcvera.core.store;
import org.springframework.data.repository.CrudRepository;
interface ConversationRepository extends CrudRepository<ConversationRow, String> { }
