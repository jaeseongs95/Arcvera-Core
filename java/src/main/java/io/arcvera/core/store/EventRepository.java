package io.arcvera.core.store;
import org.springframework.data.repository.CrudRepository;
interface EventRepository extends CrudRepository<EventRow, String> { }
