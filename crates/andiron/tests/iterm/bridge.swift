    private final class PromkitRecordingSession: FakeSession {
        var reports = Data()
        override func screenSendReport(_ data: Data) { reports.append(data) }
    }

    func testAndironScrollbackBridge() throws {
        let directory = URL(fileURLWithPath: @DIRECTORY@)
        let screen = VT100Screen()
        let session = PromkitRecordingSession()
        session.configuration.clearScrollbackAllowed = true
        session.configuration.maxScrollbackLines = 10000
        session.configuration.saveToScrollbackInAlternateScreen = true
        session.screen = screen
        screen.delegate = session
        screen.performBlock(joinedThreads: { _, state, _ in
            state.setConfig(session.configuration)
            state.terminalEnabled = true
            state.terminal!.termType = "xterm"
            state.terminal!.encoding = String.Encoding.utf8.rawValue
            screen.destructivelySetScreenWidth(96, height: 12, mutableState: state)
            state.currentGrid.cursor = VT100GridCoordMake(0, 0)
            state.maxScrollbackLines = 10000
        })
        try Data("ready".utf8).write(to: directory.appendingPathComponent("ready"))
        let deadline = Date(timeIntervalSinceNow: 900)
        for index in 0..<20000 {
            let request = directory.appendingPathComponent("request-\(index).json")
            while !FileManager.default.fileExists(atPath: request.path) {
                if Date() >= deadline { XCTFail("Native bridge driver timed out"); return }
                RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.001))
            }
            let object = try JSONSerialization.jsonObject(with: Data(contentsOf: request)) as! [String: Any]
            if object["stop"] as? Bool == true { return }
            if let width = object["width"] as? Int, let height = object["height"] as? Int {
                screen.size = VT100GridSizeMake(Int32(width), Int32(height))
            }
            if let bytes = object["data"] as? String, let data = Data(base64Encoded: bytes) {
                var buffer = Array(data)
                buffer.withUnsafeMutableBytes { bytes in
                    screen.threadedReadTask(bytes.baseAddress!.assumingMemoryBound(to: CChar.self), length: Int32(bytes.count))
                }
                screen.performBlock(joinedThreads: { _, state, _ in state.scheduleTokenExecution() })
            }
            for _ in 0..<3 {
                screen.performBlock(joinedThreads: { _, _, _ in })
                RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.001))
            }
            var history = 0
            var cursor: [Int32] = []
            var savedCursor: [Int32] = []
            var rows: [String] = []
            var allRows: [String] = []
            var updating = false
            screen.performBlock(joinedThreads: { _, state, _ in
                history = Int(state.numberOfScrollbackLines)
                updating = state.terminal!.synchronizedUpdates
                cursor = [state.currentGrid.cursor.x, state.currentGrid.cursor.y]
                savedCursor = [state.terminal!.savedCursorPosition().x, state.terminal!.savedCursorPosition().y]
                for row in 0..<Int(state.height) {
                    let cells = state.currentGrid.immutableScreenChars(atLineNumber: Int32(row))!
                    rows.append((0..<Int(state.width)).map { col in
                        let code = cells[col].code
                        return code == 0 ? " " : String(UnicodeScalar(UInt32(code))!)
                    }.joined())
                }
                for row in 0..<Int(state.numberOfLines) {
                    let cells = state.getLineAt(Int32(row))!
                    allRows.append((0..<Int(state.width)).map { col in
                        let code = cells[col].code
                        return code == 0 ? " " : String(UnicodeScalar(UInt32(code))!)
                    }.joined())
                }
            })
            let response: [String: Any] = [
                "dump": screen.compactLineDumpWithHistory(), "history": history,
                "cursor": cursor, "savedCursor": savedCursor, "rows": rows, "all_rows": allRows, "updating": updating, "reply": session.reports.base64EncodedString()
            ]
            session.reports.removeAll()
            try JSONSerialization.data(withJSONObject: response).write(
                to: directory.appendingPathComponent("response-\(index).json"), options: .atomic)
        }
        XCTFail("Native bridge exceeded request limit")
    }
