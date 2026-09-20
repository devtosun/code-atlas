// İstanbul and müşteri keep multibyte source bytes before declarations.
import 'package:ornek/veri.dart' as veri;

class Customer {
  final String name;

  Customer(this.name);

  String summary() => veri.write(name);
}
