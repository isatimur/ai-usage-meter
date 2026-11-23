import XCTest
@testable import AIUsageMeter

final class PopoverLayoutTests: XCTestCase {
    func testShortContentKeepsItsOwnHeight() {
        XCTAssertEqual(PopoverLayout.scrollHeight(content: 300, screen: 900), 300)
    }

    func testTallContentIsCappedBelowTheScreen() {
        let h = PopoverLayout.scrollHeight(content: 2000, screen: 900)
        XCTAssertLessThan(h, 900)
        XCTAssertEqual(h, 900 - PopoverLayout.reserved)
    }

    func testASmallScreenStillGetsAUsableHeight() {
        XCTAssertEqual(PopoverLayout.scrollHeight(content: 2000, screen: 300), PopoverLayout.minimum)
    }

    func testBeforeTheContentIsMeasuredTheCapApplies() {
        XCTAssertEqual(PopoverLayout.scrollHeight(content: 0, screen: 900), 900 - PopoverLayout.reserved)
    }
}
