library billing.feature;

import 'dart:async';
import 'package:external/repository.dart' as repo show Repository;
export 'invoice.dart' show Invoice;
part 'advanced_part.dart';

mixin Audited {
  String get auditLabel => 'ok';
}

enum Status { pending, paid }

typedef Mapper<T, R> = R Function(T);

sealed class Event {}

abstract class Shape {
  const Shape();
  factory Shape.circle(double radius) = Circle;
}

class Circle implements Shape {
  final double radius;
  const Circle(this.radius);
}

class InvoiceEvent<T> extends Event with Audited {
  final T value;

  InvoiceEvent(this.value);
  InvoiceEvent.named({required this.value});
  factory InvoiceEvent.from(T value) => InvoiceEvent<T>(value);

  String get label => '$value';
  set label(String ignored) {}

  Future<T> load(repo.Repository<T> repository) async {
    final local = await repository.fetch(value);
    final callback = (T shadowed) => shadowed;
    return callback(local);
  }
}

// phantom_call() and PhantomWidget() are not source calls.
