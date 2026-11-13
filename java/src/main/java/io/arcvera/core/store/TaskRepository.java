package io.arcvera.core.store;
import org.springframework.data.repository.CrudRepository;
interface TaskRepository extends CrudRepository<TaskRow, String> { }
