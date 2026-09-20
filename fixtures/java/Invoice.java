package billing;
public final class Invoice {
    private final long amount;
    public Invoice(long amount) { this.amount = amount; }
    public long total() { return Calculator.twice(amount); }
    public long total(long discount) { return total() - discount; }
}
// phantom_call() is only a comment.
