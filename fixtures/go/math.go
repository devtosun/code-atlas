package billing
func double(n int64) int64 { return n * 2 }
func Shadow() int64 {
    double := func(n int64) int64 { return n + 10 }
    return double(2)
}
func Identity[T any](value T) T { return value }
