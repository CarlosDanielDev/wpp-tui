// Package main is the cgo c-archive shim that exposes whatsmeow to the Rust
// side over a C ABI. It is built with `go build -buildmode=c-archive` and linked
// into the Rust binary when the `whatsmeow` Cargo feature is enabled.
//
// This is a compiling skeleton: it has no whatsmeow dependency yet. The real
// pairing / send / receive surface is added in the QR-login phase (P2).
package main

import "C"

//export wpp_bridge_version
func wpp_bridge_version() *C.char {
	return C.CString("0.1.0-stub")
}

func main() {}
