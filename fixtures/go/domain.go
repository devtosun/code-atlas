//go:build linux && amd64

package billing

import (
	json "encoding/json"
	"io"
	ext "example.com/external/client"
)

type Invoice[T any] struct {
	Amount T
	io.Reader
}

type Report struct {
	Title string
}

type Store[T any] interface {
	Save(value T) error
}

func (i *Invoice[T]) Total() T {
	return i.Amount
}

func (r Report) Total() string {
	return r.Title
}

func NewInvoice[T any](value T) *Invoice[T] {
	encode := func(input T) ([]byte, error) {
		return json.Marshal(input)
	}
	_, _ = encode(value)
	ext.Send("İstanbul")
	return &Invoice[T]{Amount: value}
}

func Özet() string { return "İstanbul" }

func Build() *Invoice[string] { return NewInvoice("sample") }

// phantom_call() is not executable source.
const Note = "phantom_call()"
