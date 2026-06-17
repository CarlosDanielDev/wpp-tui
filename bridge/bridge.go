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
	"strings"
	"sync"
	"sync/atomic"
	"unsafe"

	_ "github.com/mattn/go-sqlite3"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

var (
	mu        sync.Mutex
	client    *whatsmeow.Client
	connected atomic.Bool
	qrCode    atomic.Pointer[string]
	lastErr   atomic.Value
	ctx       context.Context
	cancelFn  context.CancelFunc
	msgMu     sync.Mutex
	msgQueue  []string
	presMu    sync.Mutex
	presQueue []string
	rcptMu    sync.Mutex
	rcptQueue []string
)

func setError(err error) {
	if err == nil {
		return
	}
	lastErr.Store(err.Error())
}

// canonicalChat returns a stable, phone-number-form JID string for a chat, so a
// single person is never split across their LID (@lid) and phone-number
// (@s.whatsapp.net) identities. Device (AD) suffixes are stripped; an @lid JID
// is mapped to its phone number via the LID store when a mapping is known.
// Groups, broadcast, newsletters, etc. pass through unchanged (sans device).
func canonicalChat(c *whatsmeow.Client, jid types.JID) string {
	jid = jid.ToNonAD()
	if jid.Server == types.HiddenUserServer && c.Store != nil && c.Store.LIDs != nil {
		if pn, err := c.Store.LIDs.GetPNForLID(context.Background(), jid); err == nil && !pn.IsEmpty() {
			return pn.ToNonAD().String()
		}
	}
	return jid.String()
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
		switch v := evt.(type) {
		case *events.Connected:
			connected.Store(true)
		case *events.Disconnected:
			connected.Store(false)
		case *events.Message:
			text := v.Message.GetConversation()
			if text == "" {
				if ext := v.Message.GetExtendedTextMessage(); ext != nil {
					text = ext.GetText()
				}
			}
			if text == "" {
				return // non-text message; ignore for the text-only phase
			}
			flag := "0"
			if v.Info.IsFromMe {
				flag = "1"
			}
			line := canonicalChat(c, v.Info.Chat) + "\t" + flag + "\t" + text
			msgMu.Lock()
			msgQueue = append(msgQueue, line)
			msgMu.Unlock()
		case *events.ChatPresence:
			// Typing notification. "composing" → typing; "paused" → online.
			state := "online"
			if v.State == types.ChatPresenceComposing {
				state = "typing"
			}
			line := canonicalChat(c, v.MessageSource.Chat) + "\t" + state + "\t"
			presMu.Lock()
			presQueue = append(presQueue, line)
			presMu.Unlock()
		case *events.Presence:
			// Online / offline (with optional last-seen) for a subscribed user.
			var line string
			if v.Unavailable {
				lastSeen := ""
				if !v.LastSeen.IsZero() {
					lastSeen = v.LastSeen.Local().Format("2006-01-02 15:04")
				}
				line = canonicalChat(c, v.From) + "\toffline\t" + lastSeen
			} else {
				line = canonicalChat(c, v.From) + "\tonline\t"
			}
			presMu.Lock()
			presQueue = append(presQueue, line)
			presMu.Unlock()
		case *events.Receipt:
			// Delivery / read receipt for one or more of our sent messages.
			state := ""
			switch v.Type {
			case types.ReceiptTypeDelivered:
				state = "delivered"
			case types.ReceiptTypeRead, types.ReceiptTypeReadSelf:
				state = "read"
			}
			if state == "" || len(v.MessageIDs) == 0 {
				return // playedself / other receipt types we don't surface
			}
			ids := make([]string, len(v.MessageIDs))
			for i, id := range v.MessageIDs {
				ids[i] = string(id)
			}
			line := canonicalChat(c, v.MessageSource.Chat) + "\t" + state + "\t" + strings.Join(ids, ",")
			rcptMu.Lock()
			rcptQueue = append(rcptQueue, line)
			rcptMu.Unlock()
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

			// The QR channel only emits codes once the websocket is up, so we
			// must Connect() before ranging — otherwise no code ever arrives
			// and the client sits forever on "waiting for QR code".
			if err := c.Connect(); err != nil {
				setError(fmt.Errorf("Connect: %w", err))
				cancel()
				return
			}

			for item := range qrChan {
				switch item.Event {
				case "code":
					code := item.Code
					qrCode.Store(&code)
				case "success":
					// Pairing done; the existing connection stays up.
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

//export wpp_bridge_fetch_contacts
func wpp_bridge_fetch_contacts() *C.char {
	mu.Lock()
	c := client
	mu.Unlock()

	if c == nil || c.Store.Contacts == nil {
		return nil
	}

	contacts, err := c.Store.Contacts.GetAllContacts(context.Background())
	if err != nil {
		setError(fmt.Errorf("GetAllContacts: %w", err))
		return nil
	}

	// Emit every contact under its own JID key (both phone-number and LID
	// entries). Name resolution must stay keyed by whatever JID a chat actually
	// uses, so a chat that is still LID-keyed (e.g. restored from an older store)
	// keeps its pushname instead of falling back to a raw JID. The split between
	// a person's LID and PN identities is collapsed at the message boundary
	// (see canonicalChat), not by dropping contact keys here.
	var lines []string
	for jid, info := range contacts {
		name := info.FullName
		if name == "" {
			name = info.PushName
		}
		if name == "" {
			name = info.BusinessName
		}
		if name == "" {
			name = info.FirstName
		}
		if name == "" {
			name = jid.User
		}
		lines = append(lines, jid.String()+"\t"+name)
	}

	return C.CString(strings.Join(lines, "\n"))
}

//export wpp_bridge_send_text
func wpp_bridge_send_text(idStr *C.char, jidStr *C.char, body *C.char) C.int {
	mu.Lock()
	c := client
	mu.Unlock()

	if c == nil {
		setError(fmt.Errorf("send: bridge not initialized"))
		return -1
	}

	to, err := types.ParseJID(C.GoString(jidStr))
	if err != nil {
		setError(fmt.Errorf("send: parse jid: %w", err))
		return -2
	}

	msg := &waE2E.Message{Conversation: proto.String(C.GoString(body))}
	// Stamp the message with our local id so delivery receipts can be matched
	// back to it on the Rust side.
	extra := whatsmeow.SendRequestExtra{ID: types.MessageID(C.GoString(idStr))}
	if _, err := c.SendMessage(context.Background(), to, msg, extra); err != nil {
		setError(fmt.Errorf("send: %w", err))
		return -3
	}
	return 0
}

//export wpp_bridge_poll_message
func wpp_bridge_poll_message() *C.char {
	msgMu.Lock()
	defer msgMu.Unlock()
	if len(msgQueue) == 0 {
		return nil
	}
	line := msgQueue[0]
	msgQueue = msgQueue[1:]
	return C.CString(line)
}

//export wpp_bridge_poll_presence
func wpp_bridge_poll_presence() *C.char {
	presMu.Lock()
	defer presMu.Unlock()
	if len(presQueue) == 0 {
		return nil
	}
	line := presQueue[0]
	presQueue = presQueue[1:]
	return C.CString(line)
}

//export wpp_bridge_poll_receipt
func wpp_bridge_poll_receipt() *C.char {
	rcptMu.Lock()
	defer rcptMu.Unlock()
	if len(rcptQueue) == 0 {
		return nil
	}
	line := rcptQueue[0]
	rcptQueue = rcptQueue[1:]
	return C.CString(line)
}

//export wpp_bridge_subscribe_presence
func wpp_bridge_subscribe_presence(jidStr *C.char) C.int {
	mu.Lock()
	c := client
	mu.Unlock()

	if c == nil {
		setError(fmt.Errorf("subscribe_presence: bridge not initialized"))
		return -1
	}

	jid, err := types.ParseJID(C.GoString(jidStr))
	if err != nil {
		setError(fmt.Errorf("subscribe_presence: parse jid: %w", err))
		return -2
	}

	if err := c.SubscribePresence(context.Background(), jid); err != nil {
		setError(fmt.Errorf("subscribe_presence: %w", err))
		return -3
	}
	return 0
}

func main() {}
