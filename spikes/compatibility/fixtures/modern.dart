extension type UserId(int value) {
  bool get isPositive => value > 0;
}

(UserId, String) describe(int raw) {
  final id = UserId(raw);
  final label = switch (raw) {
    > 0 => 'positive',
    _ => 'other',
  };
  return (id, label);
}
