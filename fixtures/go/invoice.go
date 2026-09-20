package billing
type Invoice struct { Amount int64 }
func (i *Invoice) Total() int64 { return double(i.Amount) }
func Summarize(i *Invoice) int64 { return i.Total() }
type Repository interface { Save(i Invoice) error }
func Persist(r Repository, i Invoice) error { return r.Save(i) }
// phantom_call() is not executable source.
