namespace Billing;
public partial class Invoice
{
    public decimal Amount { get; init; }
    public decimal Total() => Calculator.Double(Amount);
    public decimal Total(decimal discount) => Total() - discount;
}
public static class Calculator
{
    public static decimal Double(decimal amount) => amount * 2m;
}
public interface IRepository { void Save(Invoice invoice); }
// phantom_call() should never create an edge.
