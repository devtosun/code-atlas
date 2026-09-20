sealed class Outcome {}
final class Success extends Outcome {}
extension type InvoiceId(int value) {}
(int, String) summary(int amount) => (amount, 'TRY');
String currencyOf((int, String) value) {
  return switch (value) { (final amount, final currency) => currency };
}
