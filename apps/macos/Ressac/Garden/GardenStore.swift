import GRDB
import SwiftUI

/// The garden's state machine, shared by both looks (spec §7, §8): the rows the list draws, the
/// queue's memory, the fetch log of the selected feed, the last refusal, the selection logic and
/// the command dispatch over `GardenModel`. `GardenTable` (Native) and `AntiopeRootView`
/// (Antiope, one store shared by the garden pages and the Minitel bar) each own one.
@MainActor @Observable final class GardenStore {
    /// `Queries.garden(scope: .all)` (active feeds) and `.cemetery` (buried ones, kept in All Feeds
    /// and Needs Triage only when buried during this visit).
    private(set) var active: [GardenRow] = []
    private(set) var buried: [GardenRow] = []
    /// Feeds seen needing triage during this visit: once decided (here, by ⇧R or in the
    /// terminal) they stay listed, dimmed, until the list is left.
    private(set) var seenInQueue: Set<Int64> = []
    private(set) var log: [FetchLogRow] = []
    /// The last refusal ("Nothing to repair for this feed."), shown in the header or Minitel bar.
    private(set) var message: String?
    private(set) var confirmingSafeRepairs = false

    var rows: [GardenRow] { active + buried }

    func retained(_ app: AppState) -> Set<Int64> {
        seenInQueue.union(app.decidedThisVisit)
    }

    func selectedRow(_ app: AppState) -> GardenRow? {
        rows.first { $0.feedId == app.selectedFeedId }
    }

    func clearMessage() {
        message = nil
    }

    func confirmSafeRepairs() {
        confirmingSafeRepairs = true
    }

    /// The confirmation dialog's setter (it only ever dismisses).
    func setConfirmingSafeRepairs(_ confirming: Bool) {
        confirmingSafeRepairs = confirming
    }

    /// Starts a visit of the page `scope` and observes the lists it needs until the task is
    /// cancelled (the page changes or disappears).
    func observe(page scope: GardenScope, app: AppState) async {
        active = []
        buried = []
        seenInQueue = []
        message = nil
        await withDiscardingTaskGroup { group in
            if scope != .cemetery {
                group.addTask { await self.follow(.all, page: scope, app: app) }
            }
            group.addTask { await self.follow(.cemetery, page: scope, app: app) }
        }
    }

    func observeLog(feedId: Int64?, app: AppState) async {
        log = []
        guard let feedId else { return }
        do {
            for try await fresh in Queries.fetchLog(feedId: feedId).values(in: app.database.reader) {
                log = fresh
            }
        } catch {
            log = []
        }
    }

    /// Carries out a key's command: the terminal garden's keys, in both looks.
    func perform(_ command: GardenCommand, app: AppState) {
        message = nil
        switch command {
        case .decide(let decision): app.gardenDecide(decision)
        case .undo: app.gardenUndo()
        case .open(let url): app.openURL(url)
        case .confirmSafeRepairs: confirmingSafeRepairs = true
        case .message(let text): message = text
        }
    }

    private func follow(_ query: GardenScope, page scope: GardenScope, app: AppState) async {
        do {
            for try await fresh in Queries.garden(scope: query).values(in: app.database.reader) {
                let previous = rows
                if query == .cemetery {
                    buried = fresh
                } else {
                    active = fresh
                }
                rowsDidChange(previous: previous, scope: scope, app: app)
            }
        } catch {
            if !Task.isCancelled {
                message = "The garden could not be read."
            }
        }
    }

    /// Keeps the queue's memory and moves the selection on when the selected feed was decided.
    private func rowsDidChange(previous: [GardenRow], scope: GardenScope, app: AppState) {
        seenInQueue.formUnion(rows.filter(\.needsTriage).map(\.feedId))
        let ordered = GardenModel.sections(
            GardenModel.visibleRows(rows, scope: scope, seenInQueue: retained(app)), scope: scope
        ).flatMap(\.rows)
        if scope == .needsTriage {
            app.selectedFeedId = GardenModel.selectionAfterUpdate(
                selected: app.selectedFeedId, previous: previous, current: rows, ordered: ordered)
        }
        if !ordered.contains(where: { $0.feedId == app.selectedFeedId }) {
            app.selectedFeedId = ordered.first(where: \.needsTriage)?.feedId ?? ordered.first?.feedId
        }
    }
}
