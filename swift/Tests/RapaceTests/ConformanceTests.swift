import XCTest
import Foundation
@testable import Rapace
@testable import ConformanceRunner

/// Conformance tests for Swift Rapace implementation.
///
/// These tests run the Swift implementation against the
/// `rapace-conformance` reference peer to validate spec compliance.
final class ConformanceTests: XCTestCase {

    /// Path to the conformance binary
    static var conformanceBinary: String {
        // Try to find in target/debug first, then release
        let workspaceRoot = findWorkspaceRoot()
        let debugPath = workspaceRoot.appendingPathComponent("target/debug/rapace-conformance").path
        let releasePath = workspaceRoot.appendingPathComponent("target/release/rapace-conformance").path
        
        if FileManager.default.fileExists(atPath: debugPath) {
            return debugPath
        } else if FileManager.default.fileExists(atPath: releasePath) {
            return releasePath
        }
        return "rapace-conformance" // Fall back to PATH
    }

    /// Find workspace root (where Cargo.toml is)
    static func findWorkspaceRoot() -> URL {
        var dir = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<10 {
            let cargoToml = dir.appendingPathComponent("Cargo.toml")
            if FileManager.default.fileExists(atPath: cargoToml.path) {
                return dir
            }
            let parent = dir.deletingLastPathComponent()
            if parent == dir { break }
            dir = parent
        }
        // Default fallback
        return URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    }

    /// Check if the conformance binary is available
    static func conformanceBinaryExists() -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: conformanceBinary)
        process.arguments = ["--list"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }

    // MARK: - Handshake Tests

    func testHandshakeValidHelloExchange() throws {
        try XCTSkipUnless(Self.conformanceBinaryExists(), "rapace-conformance binary not found")

        let passed = runConformanceTest(
            "handshake.valid_hello_exchange",
            binary: Self.conformanceBinary
        ) { runner in
            try runner.doHandshakeAsInitiator()
        }

        XCTAssertTrue(passed, "handshake.valid_hello_exchange should pass")
    }

    // MARK: - Frame Format Tests

    func testFrameDescriptorSize() throws {
        // This test doesn't need the binary - just validates MsgDescHot is 64 bytes
        let desc = MsgDescHot()
        let serialized = desc.serialize()
        XCTAssertEqual(serialized.count, 64, "MsgDescHot should serialize to 64 bytes")
    }

    func testFrameEncodingLittleEndian() throws {
        // Verify little-endian encoding
        var desc = MsgDescHot()
        desc.msgId = 0x0102030405060708
        desc.channelId = 0x11121314
        desc.methodId = 0x21222324

        let bytes = desc.serialize()

        // msgId at offset 0, little-endian
        XCTAssertEqual(bytes[0], 0x08)
        XCTAssertEqual(bytes[7], 0x01)

        // channelId at offset 8, little-endian
        XCTAssertEqual(bytes[8], 0x14)
        XCTAssertEqual(bytes[11], 0x11)

        // methodId at offset 12, little-endian
        XCTAssertEqual(bytes[12], 0x24)
        XCTAssertEqual(bytes[15], 0x21)
    }

    func testFrameSentinelInline() throws {
        XCTAssertEqual(inlinePayloadSlot, 0xFFFFFFFF, "Inline payload sentinel should be 0xFFFFFFFF")
    }

    // MARK: - Method ID Tests

    func testMethodIdDeterministic() throws {
        let id1 = computeMethodId(service: "Calculator", method: "add")
        let id2 = computeMethodId(service: "Calculator", method: "add")
        XCTAssertEqual(id1, id2, "Same input should produce same output")
    }

    func testMethodIdDifferentMethods() throws {
        let id1 = computeMethodId(service: "Calculator", method: "add")
        let id2 = computeMethodId(service: "Calculator", method: "subtract")
        XCTAssertNotEqual(id1, id2, "Different methods should produce different IDs")
    }

    func testMethodIdNonZero() throws {
        let id = computeMethodId(service: "Test", method: "method")
        XCTAssertNotEqual(id, 0, "Method IDs should not be 0 (reserved for control)")
    }
}
