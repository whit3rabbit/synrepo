package main

import (
	"fmt"
	"os"
)

// Greeter holds configuration for greeting users.
type Greeter struct {
	Name   string
	Prefix string
}

// Namer is an interface for objects that provide a name.
type Namer interface {
	GetName() string
	SetName(name string)
}

// MaxRetries is the maximum number of retry attempts.
const MaxRetries = 3

// Greet returns a greeting string for the given name.
func Greet(name string) string {
	return fmt.Sprintf("Hello, %s!", name)
}

// GetName returns the name of the Greeter.
func (g *Greeter) GetName() string {
	return g.Name
}

// SetName sets the name of the Greeter.
func (g *Greeter) SetName(name string) {
	g.Name = name
}

func main() {
	g := &Greeter{Name: "World"}
	fmt.Println(Greet(g.GetName()))
	_ = os.Args
}
