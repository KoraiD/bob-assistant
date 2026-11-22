import Testing
@testable import BobNativeCore

@Suite("BobNativeInfo")
struct BobNativeInfoTests {
    @Test func bannerContainsNameAndProtocolVersion() {
        let banner = BobNativeInfo.banner()
        #expect(banner.contains(BobNativeInfo.name))
        #expect(banner.contains("protocol=\(BobNativeInfo.protocolVersion)"))
    }

    @Test func protocolVersionIsNonNegative() {
        #expect(BobNativeInfo.protocolVersion >= 0)
    }
}
