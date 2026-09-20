package billing.workflow;

import java.util.List;
import java.util.function.Function;
import static java.util.Objects.requireNonNull;

@interface Audited {
    String value();
}

interface Repository<T> {
    void save(T value);
}

enum InvoiceState { DRAFT, SENT }

record Entry<T>(T value) { }

@Audited("billing")
final class Workflow<T> {
    private final Repository<T> repository;

    Workflow(Repository<T> repository) {
        this.repository = requireNonNull(repository);
    }

    <R> R map(T value, Function<T, R> mapper) {
        return mapper.apply(value);
    }

    <R> R map(T value, R fallback, Function<T, R> mapper) {
        return value == null ? fallback : mapper.apply(value);
    }

    void persist(T value) {
        repository.save(value);
        Function<T, T> identity = item -> item;
        Function<T, Entry<T>> constructor = Entry::new;
        identity.apply(constructor.apply(value).value());
    }

    static final class Nested {
        Invoice make(long amount) { return new Invoice(amount); }
    }
}

