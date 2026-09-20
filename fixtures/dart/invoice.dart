import 'math.dart' as calc;
class Invoice {
  final int amount;
  Invoice(this.amount);
  Invoice.zero() : amount = 0;
  factory Invoice.from(int amount) => Invoice(amount);
  int get total => calc.twice(amount);
  String label() => 'phantom_call()';
}
extension InvoiceLabel on Invoice {
  String display() => 'Amount: $amount';
}
// phantom_call() is not a source call.
