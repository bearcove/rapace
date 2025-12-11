# Page snapshot

```yaml
- generic [active] [ref=e1]:
  - heading "rapace Explorer Demo" [level=1] [ref=e2]
  - generic [ref=e3]:
    - heading "Connection Connected" [level=2] [ref=e4]:
      - text: Connection
      - generic [ref=e5]: Connected
    - textbox "WebSocket URL" [disabled] [ref=e6]: ws://127.0.0.1:4268
    - button "Connect" [disabled] [ref=e7]
    - button "Disconnect" [ref=e8] [cursor=pointer]
  - generic [ref=e9]:
    - heading "Services" [level=2] [ref=e10]
    - generic [ref=e11]:
      - generic [ref=e12] [cursor=pointer]:
        - strong [ref=e13]: Calculator
        - text: 3 methods
      - generic [ref=e14] [cursor=pointer]:
        - strong [ref=e15]: Greeter
        - text: 2 methods
      - generic [ref=e16] [cursor=pointer]:
        - strong [ref=e17]: Counter
        - text: 2 methods
    - button "Refresh Services" [ref=e18] [cursor=pointer]
  - generic [ref=e19]:
    - heading "Log" [level=2] [ref=e20]
    - button "Clear" [ref=e21] [cursor=pointer]
    - generic [ref=e22]: "[8:36:08 AM] Page loaded. Enter WebSocket URL and click \"Connect\" to start. [8:36:09 AM] Initializing WASM module... [8:36:09 AM] Connecting to ws://127.0.0.1:4268... [8:36:09 AM] Connected! [8:36:09 AM] Discovering services... [8:36:09 AM] Found 3 service(s)"
```