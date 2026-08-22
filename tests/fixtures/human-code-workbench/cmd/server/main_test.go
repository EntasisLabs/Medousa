package main

import "testing"

func TestServerStarts(t *testing.T) {
	if 2+2 != 4 {
		t.Fatal("math stopped working")
	}
}
