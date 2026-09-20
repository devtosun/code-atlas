namespace Billing;
public partial class Invoice
{
    public string Customer { get; init; } = "";
}
public record InvoiceSummary(decimal Amount, string Customer);
