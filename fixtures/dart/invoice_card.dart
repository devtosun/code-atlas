import 'package:flutter/widgets.dart';
import 'invoice.dart';
class InvoiceCard extends StatelessWidget {
  final Invoice invoice;
  const InvoiceCard({super.key, required this.invoice});
  @override
  Widget build(BuildContext context) {
    return Column(children: [Text(invoice.label()), Text('${invoice.total}')]);
  }
}
