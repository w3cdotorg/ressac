import AppKit
import Foundation
import GRDB
import Synchronization

/// Detects commits made by other connections (a `ressac` run in a terminal) with
/// `PRAGMA data_version`, which changes only for other connections' commits and costs next to
/// nothing. The value is per connection, so the watcher keeps one connection of its own.
/// GRDB's observations only see this app's writes; the owner turns a change into
/// `AppDatabase.notifyExternalChanges()`.
final class DataVersionWatcher: Sendable {
    private let queue: DatabaseQueue
    private let last = Mutex<Int?>(nil)

    init(url: URL) throws {
        var config = Configuration()
        config.readonly = true
        config.label = "Ressac data_version"
        queue = try DatabaseQueue(path: url.path, configuration: config)
    }

    private func version() throws -> Int {
        try queue.read { db in try Int.fetchOne(db, sql: "PRAGMA data_version") ?? 0 }
    }

    /// Remembers the current version without reporting it.
    func prime() throws {
        let current = try version()
        last.withLock { $0 = current }
    }

    /// True when another connection committed since the previous call (or since `prime()`).
    /// The first call after creation only records the baseline.
    func hasChanged() throws -> Bool {
        let current = try version()
        return last.withLock { previous in
            defer { previous = current }
            return previous.map { $0 != current } ?? false
        }
    }
}

/// Keeps the open window in step with writes made elsewhere: it refreshes when the app
/// becomes active again, and while it is active it polls `data_version` every few seconds.
@MainActor final class ExternalChangeMonitor {
    private let database: AppDatabase
    private let watcher: DataVersionWatcher?
    private let interval: Duration
    private let isActive: @MainActor () -> Bool
    private var loop: Task<Void, Never>?
    private var observer: (any NSObjectProtocol)?

    init(database: AppDatabase, interval: Duration = .seconds(3),
         isActive: @escaping @MainActor () -> Bool = { NSApp?.isActive ?? false }) {
        self.database = database
        self.watcher = try? DataVersionWatcher(url: database.url)
        self.interval = interval
        self.isActive = isActive
    }

    func start() {
        guard loop == nil else { return }
        try? watcher?.prime()
        observer = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.didBecomeActive() }
        }
        loop = Task { [weak self, interval] in
            while !Task.isCancelled {
                try? await Task.sleep(for: interval)
                guard let self, !Task.isCancelled else { return }
                await pollIfActive()
            }
        }
    }

    func stop() {
        loop?.cancel()
        loop = nil
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil
    }

    /// Switching back from Terminal: refetch everything, and take a fresh baseline.
    func didBecomeActive() {
        try? watcher?.prime()
        Task { try? await database.notifyExternalChanges() }
    }

    private func pollIfActive() async {
        guard isActive() else { return }
        await poll()
    }

    /// One poll. Returns true (after notifying the observations) when another connection committed.
    @discardableResult
    func poll() async -> Bool {
        guard (try? watcher?.hasChanged()) == true else { return false }
        try? await database.notifyExternalChanges()
        return true
    }
}
