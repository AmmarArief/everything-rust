// Package strkit runs the Go original and the Rust port of `strkit/` side by side. Generated
// by carcinize: import it in place of example.com/strkit/strkit. CARCINIZE_MODE=old|shadow|new.
package strkit

import (
	"encoding/json"
	"log"
	"os"
	"reflect"
	"sync/atomic"

	port "carcinize.local/strkit_rs"
	orig "example.com/strkit/strkit"
)

var mode = func() string {
	if m := os.Getenv("CARCINIZE_MODE"); m != "" {
		return m
	}
	return "shadow"
}()

// Calls and Mismatches count shadowed calls.
var Calls, Mismatches atomic.Int64

// convert moves a value between the original's and the wrapper's identical types.
func convert[T any](v any) T {
	var out T
	if text, err := json.Marshal(v); err == nil {
		_ = json.Unmarshal(text, &out)
	}
	return out
}

func compare(name string, expected, actual []any) {
	Calls.Add(1)
	if !reflect.DeepEqual(expected, actual) {
		if Mismatches.Add(1) <= 10 {
			log.Printf("[carcinize] mismatch in %s: expected %v, got %v", name, expected, actual)
		}
	}
}

// Point is the original's type.
type Point = orig.Point

// Slugify shadows the original `Slugify`.
func Slugify(s string, max int) string {
	if mode == "new" {
		n0 := port.Slugify(s, max)
		return n0
	}
	o0 := orig.Slugify(s, max)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("Slugify", []any{"no panic"}, []any{r})
				}
			}()
			n0 := port.Slugify(s, max)
			compare("Slugify", []any{o0}, []any{n0})
		}()
	}
	return o0
}

// WordFreq shadows the original `WordFreq`.
func WordFreq(text string) map[string]int {
	if mode == "new" {
		n0 := port.WordFreq(text)
		return n0
	}
	o0 := orig.WordFreq(text)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("WordFreq", []any{"no panic"}, []any{r})
				}
			}()
			n0 := port.WordFreq(text)
			compare("WordFreq", []any{o0}, []any{n0})
		}()
	}
	return o0
}

// ParsePort shadows the original `ParsePort`.
func ParsePort(s string) (int, error) {
	if mode == "new" {
		n0, n1 := port.ParsePort(s)
		return n0, n1
	}
	o0, o1 := orig.ParsePort(s)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("ParsePort", []any{"no panic"}, []any{r})
				}
			}()
			n0, n1 := port.ParsePort(s)
			compare("ParsePort", []any{o0, errText(o1)}, []any{n0, errText(n1)})
		}()
	}
	return o0, o1
}

// Mean shadows the original `Mean`.
func Mean(xs []float64) float64 {
	if mode == "new" {
		n0 := port.Mean(xs)
		return n0
	}
	o0 := orig.Mean(xs)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("Mean", []any{"no panic"}, []any{r})
				}
			}()
			n0 := port.Mean(xs)
			compare("Mean", []any{o0}, []any{n0})
		}()
	}
	return o0
}

// Midpoint shadows the original `Midpoint`.
func Midpoint(a Point, b Point) Point {
	if mode == "new" {
		n0 := port.Midpoint(convert[port.Point](a), convert[port.Point](b))
		return convert[Point](n0)
	}
	o0 := orig.Midpoint(a, b)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("Midpoint", []any{"no panic"}, []any{r})
				}
			}()
			n0 := port.Midpoint(convert[port.Point](a), convert[port.Point](b))
			compare("Midpoint", []any{o0}, []any{convert[Point](n0)})
		}()
	}
	return o0
}

// Initials shadows the original `Initials`.
func Initials(name string) string {
	if mode == "new" {
		n0 := port.Initials(name)
		return n0
	}
	o0 := orig.Initials(name)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("Initials", []any{"no panic"}, []any{r})
				}
			}()
			n0 := port.Initials(name)
			compare("Initials", []any{o0}, []any{n0})
		}()
	}
	return o0
}

// SplitPair shadows the original `SplitPair`.
func SplitPair(s string, sep string) (string, string, bool) {
	if mode == "new" {
		n0, n1, n2 := port.SplitPair(s, sep)
		return n0, n1, n2
	}
	o0, o1, o2 := orig.SplitPair(s, sep)
	if mode != "old" {
		func() {
			defer func() {
				if r := recover(); r != nil {
					compare("SplitPair", []any{"no panic"}, []any{r})
				}
			}()
			n0, n1, n2 := port.SplitPair(s, sep)
			compare("SplitPair", []any{o0, o1, o2}, []any{n0, n1, n2})
		}()
	}
	return o0, o1, o2
}

func errText(err error) any {
	if err == nil {
		return nil
	}
	return err.Error()
}
