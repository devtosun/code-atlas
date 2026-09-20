using Billing;

var invoice = new Invoice();
var total = invoice.WithTax(1.2m);
Console.WriteLine(total);
var routeName = "IRepository.Save";
// phantom_call() and new Phantom() are comments, not call sites.

