import SwiftUI

/// The Antiope window (spec §5). A stub until phase 8 brings the page header, the three
/// columns and the Minitel bar: one centred line on black, in Bedstead at Antiope's own size.
/// It shares `state` with `MainView`, so switching looks loses nothing.
struct AntiopeRootView: View {
    let state: AppState
    @AppStorage(ReaderStyle.Keys.antiopeSize) private var size = TeletextFont.defaultSize

    var body: some View {
        let teletextSize = TeletextFont.snapped(size)
        ZStack {
            TeletextColor.black.color
            Text("RESSAC ANTIOPE")
                .font(TeletextFont.font(size: teletextSize))
                .foregroundStyle(TeletextColor.white.color)
        }
        .ignoresSafeArea()
        .environment(\.teletextSize, teletextSize)
    }
}
