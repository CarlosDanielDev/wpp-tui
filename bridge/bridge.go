// Package main is the cgo c-archive shim that exposes whatsmeow to the Rust
// side over a C ABI. It is built with `go build -buildmode=c-archive` and linked
// into the Rust binary when the `whatsmeow` Cargo feature is enabled.
//
// Exports functions for pairing (QR), session persistence (SQLite), and
// connection management.
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types/events"
	_ "github.com/mattn/go-sqlite3"
)

var (
	mu        sync.Mutex
	client    *whatsmeow.Client
	connected atomic.Bool
	qrCode    atomic.Pointer[string]
	lastErr   atomic.Value
	ctx       context.Context
	cancelFn  context.CancelFunc
)

func setError(err error) {
	if err == nil {
		return
	}
	lastErr.Store(err.Error())
}

//export wpp_bridge_version
func wpp_bridge_version() *C.char {
	return C.CString("0.2.0")
}

//export wpp_bridge_last_error
func wpp_bridge_last_error() *C.char {
	val := lastErr.Load()
	if val == nil {
		return nil
	}
	s, ok := val.(string)
	if !ok || s == "" {
		return nil
	}
	return C.CString(s)
}

//export wpp_bridge_init
func wpp_bridge_init(dataDir *C.char) C.int {
	wpp_bridge_disconnect()

	dir := C.GoString(dataDir)
	dsn := "file:" + dir + "/session.db?_foreign_keys=on"

	initCtx := context.Background()
	container, err := sqlstore.New(initCtx, "sqlite3", dsn, nil)
	if err != nil {
		setError(fmt.Errorf("sqlstore.New: %w", err))
		return -1
	}

	deviceStore, err := container.GetFirstDevice(initCtx)
	if err != nil {
		setError(fmt.Errorf("GetFirstDevice: %w", err))
		return -2
	}

	c := whatsmeow.NewClient(deviceStore, nil)

	c.AddEventHandler(func(evt interface{}) {
		switch evt.(type) {
		case *events.Connected:
			connected.Store(true)
		case *events.Disconnected:
			connected.Store(false)
		}
	})

	mu.Lock()
	client = c
	connected.Store(false)
	mu.Unlock()

	return 0
}

//export wpp_bridge_start
func wpp_bridge_start() C.int {
	mu.Lock()
	c := client
	mu.Unlock()

	if c == nil {
		setError(fmt.Errorf("bridge not initialized"))
		return -1
	}

	bgCtx, cancel := context.WithCancel(context.Background())
	ctx = bgCtx
	cancelFn = cancel

	if c.Store.ID == nil {
		go func() {
			qrChan, err := c.GetQRChannel(ctx)
			if err != nil {
				setError(fmt.Errorf("GetQRChannel: %w", err))
				cancel()
				return
			}

			var paired bool
			for item := range qrChan {
				switch item.Event {
				case "code":
					code := item.Code
					qrCode.Store(&code)
				case "success":
					paired = true
				case "error":
					setError(fmt.Errorf("pairing error: %w", item.Error))
					cancel()
					return
				case "timeout":
					setError(fmt.Errorf("pairing timed out"))
					cancel()
					return
				case "err-unexpected-state":
					setError(fmt.Errorf("pairing failed: unexpected state"))
					cancel()
					return
				case "err-client-outdated":
					setError(fmt.Errorf("pairing failed: client outdated"))
					cancel()
					return
				case "err-scanned-without-multidevice":
					setError(fmt.Errorf("pairing failed: scanned without multi-device"))
					cancel()
					return
				}
			}

			if paired {
				if err := c.Connect(); err != nil {
					setError(fmt.Errorf("Connect after pairing: %w", err))
				}
			}
		}()
		return 1
	}

	err := c.Connect()
	if err != nil {
		setError(fmt.Errorf("Connect: %w", err))
		return -2
	}

	return 0
}

//export wpp_bridge_poll_qr
func wpp_bridge_poll_qr() *C.char {
	ptr := qrCode.Swap(nil)
	if ptr == nil {
		return nil
	}
	return C.CString(*ptr)
}

//export wpp_bridge_is_connected
func wpp_bridge_is_connected() C.int {
	if connected.Load() {
		return 1
	}
	return 0
}

//export wpp_bridge_disconnect
func wpp_bridge_disconnect() {
	if cancelFn != nil {
		cancelFn()
		cancelFn = nil
	}

	mu.Lock()
	c := client
	client = nil
	mu.Unlock()

	if c != nil {
		c.Disconnect()
		connected.Store(false)
	}
}

//export wpp_bridge_free_string
func wpp_bridge_free_string(s *C.char) {
	C.free(unsafe.Pointer(s))
}

func main() {}
