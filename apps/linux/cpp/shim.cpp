#include "shim.h"

#include "ressac-kde/src/lib.cxx.h"

#include <QtCore/QBuffer>
#include <QtCore/QByteArray>
#include <QtCore/QCoreApplication>
#include <QtCore/QElapsedTimer>
#include <QtCore/QLoggingCategory>
#include <QtCore/QMetaObject>
#include <QtCore/QMultiMap>
#include <QtCore/QPointer>
#include <QtCore/QSettings>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QTimer>
#include <QtCore/QVariant>
#include <QtGui/QClipboard>
#include <QtGui/QFontDatabase>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QKeyEvent>
#include <QtGui/QPainter>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>
#include <QtWebEngineCore/QWebEngineUrlRequestJob>
#include <QtWebEngineCore/QWebEngineUrlScheme>
#include <QtWebEngineCore/QWebEngineUrlSchemeHandler>
#include <QtWebEngineQuick/QQuickWebEngineProfile>
#include <QtWebEngineQuick/QtWebEngineQuick>
#include <QtWidgets/QApplication>

#include <algorithm>
#include <mutex>
#include <unordered_map>
#include <vector>

Q_LOGGING_CATEGORY(lcMosaic, "ressac.mosaic")

namespace {

QString from_rust(rust::Str s)
{
    return QString::fromUtf8(s.data(), static_cast<qsizetype>(s.size()));
}

rust::String to_rust(const QString &s)
{
    const QByteArray utf8 = s.toUtf8();
    return rust::String(utf8.constData(), static_cast<std::size_t>(utf8.size()));
}

QQuickWindow *first_quick_window(bool visible_only)
{
    for (QWindow *w : QGuiApplication::topLevelWindows()) {
        if (auto *qw = qobject_cast<QQuickWindow *>(w)) {
            if (!visible_only || qw->isVisible()) {
                return qw;
            }
        }
    }
    return nullptr;
}

// The family Qt gave the registered bedstead.otf; empty when registration failed.
QString g_bedstead_family;

// The frame-time log: the intervals between the frames of the first visible window.
struct FrameLog
{
    QMetaObject::Connection conn;
    QElapsedTimer clock;
    std::mutex lock;
    std::vector<double> ms;
    bool primed = false;
};
FrameLog g_frames;

QSettings open(rust::Str file)
{
    return QSettings(from_rust(file), QSettings::IniFormat);
}

// Serves ressac:page/<entry> and ressac:asset/<file> from Rust, on the GUI thread.
class RessacSchemeHandler : public QWebEngineUrlSchemeHandler
{
public:
    using QWebEngineUrlSchemeHandler::QWebEngineUrlSchemeHandler;

    void requestStarted(QWebEngineUrlRequestJob *job) override
    {
        const std::string url = job->requestUrl().toString(QUrl::FullyEncoded).toStdString();
        rust::String mime;
        rust::Vec<std::uint8_t> body;
        if (!ressac_scheme_serve(rust::Str(url.data(), url.size()), mime, body)) {
            job->fail(QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }
        auto *buffer = new QBuffer(job);
        buffer->setData(QByteArray(reinterpret_cast<const char *>(body.data()),
                                   static_cast<qsizetype>(body.size())));
        buffer->open(QIODevice::ReadOnly);
        job->reply(QByteArray(mime.data(), static_cast<qsizetype>(mime.size())), buffer);
    }
};

// Serves ressac-mosaic:image?cols=<n>&src=<original>: each request gets a token and goes to Rust
// (`ressac_mosaic_start`), which answers later, possibly at once, through `ressac_mosaic_reply`
// with PNG bytes it produced. The pending jobs are held by QPointer: a request the page cancels
// is deleted by WebEngine, its entry removed on `destroyed` and Rust told
// (`ressac_mosaic_cancel`), and an answer that still comes is dropped.
class MosaicHandler : public QWebEngineUrlSchemeHandler
{
public:
    using QWebEngineUrlSchemeHandler::QWebEngineUrlSchemeHandler;

    void requestStarted(QWebEngineUrlRequestJob *job) override
    {
        const std::uint64_t token = ++m_next;
        m_jobs.emplace(token, QPointer<QWebEngineUrlRequestJob>(job));
        // A job WebEngine deletes before its answer was cancelled by its page: Rust drops it
        // from the mosaic queue (or stops its download).
        QObject::connect(job, &QObject::destroyed, this, [this, token] {
            if (m_jobs.erase(token) > 0) {
                ressac_mosaic_cancel(token);
            }
        });
        const std::string url = job->requestUrl().toString(QUrl::FullyEncoded).toStdString();
        ressac_mosaic_start(token, rust::Str(url.data(), url.size()));
    }

    void reply(std::uint64_t token, rust::Str mime, rust::Slice<const std::uint8_t> body)
    {
        QPointer<QWebEngineUrlRequestJob> job;
        if (const auto found = m_jobs.find(token); found != m_jobs.end()) {
            job = found->second;
            m_jobs.erase(found);
        }
        if (job.isNull()) {
            qCDebug(lcMosaic, "mosaic: dropped %llu", static_cast<unsigned long long>(token));
            return;
        }
        QObject::disconnect(job, &QObject::destroyed, this, nullptr);
        // Fixed headers only: nothing from the request is reflected into the answer.
        QMultiMap<QByteArray, QByteArray> headers;
        headers.insert(QByteArrayLiteral("Cache-Control"), QByteArrayLiteral("no-store"));
        job->setAdditionalResponseHeaders(headers);
        auto *buffer = new QBuffer(job);
        buffer->setData(QByteArray(reinterpret_cast<const char *>(body.data()),
                                   static_cast<qsizetype>(body.size())));
        buffer->open(QIODevice::ReadOnly);
        job->reply(QByteArray(mime.data(), static_cast<qsizetype>(mime.size())), buffer);
    }

private:
    std::uint64_t m_next = 0;
    std::unordered_map<std::uint64_t, QPointer<QWebEngineUrlRequestJob>> m_jobs;
};

// The installed handler (owned by the application object).
QPointer<MosaicHandler> g_mosaic_handler;

// ressac-toggle:<n> is a mosaic box's link: registered so a click on it is an ordinary navigation
// that reaches the reader's onNavigationRequested, which always rejects it (and swaps the image).
// No request is ever expected here; one that comes is refused.
class ToggleHandler : public QWebEngineUrlSchemeHandler
{
public:
    using QWebEngineUrlSchemeHandler::QWebEngineUrlSchemeHandler;

    void requestStarted(QWebEngineUrlRequestJob *job) override
    {
        job->fail(QWebEngineUrlRequestJob::RequestDenied);
    }
};

// Takes the reading keys before the focused item sees them, WebEngine's delegate included (QML
// `Keys` on an ancestor only sees what Chromium declined, a round trip later). The ShortcutOverride
// is accepted for a key the router claims, so no single-letter Action or Shortcut fires; the
// KeyPress then asks `readingPage.perform(verb)` and is consumed when it acted.
class ReadingKeyFilter : public QObject
{
public:
    ReadingKeyFilter(QQuickWindow *window, QObject *parent)
        : QObject(parent)
        , m_window(window)
    {
    }

    bool eventFilter(QObject *obj, QEvent *e) override
    {
        if (obj != m_window || (e->type() != QEvent::ShortcutOverride && e->type() != QEvent::KeyPress)) {
            return false;
        }
        QQuickItem *page = readingPage();
        if (page == nullptr) {
            return false;
        }
        const auto *k = static_cast<QKeyEvent *>(e);
        const std::string text = k->text().toStdString();
        const std::string chain = focusChain().toStdString();
        const std::string look = page->property("look").toString().toStdString();
        rust::String verb;
        const bool claims = ressac_route_key(k->key(), rust::Str(text.data(), text.size()),
                                             static_cast<std::uint32_t>(k->modifiers()), k->isAutoRepeat(),
                                             rust::Str(chain.data(), chain.size()),
                                             rust::Str(look.data(), look.size()), verb);
        if (e->type() == QEvent::ShortcutOverride) {
            if (claims) {
                e->accept();
                return true;
            }
            return false;
        }
        if (verb.empty()) {
            return false;
        }
        QVariant result;
        const QString name = QString::fromUtf8(verb.data(), static_cast<qsizetype>(verb.size()));
        QMetaObject::invokeMethod(page, "perform", Qt::DirectConnection, Q_RETURN_ARG(QVariant, result),
                                  Q_ARG(QVariant, QVariant(name)));
        return result.toBool();
    }

private:
    // The QQuickItem named `readingPage`, depth first from the content item.
    QQuickItem *readingPage()
    {
        if (m_page == nullptr) {
            m_page = find(m_window->contentItem());
        }
        return m_page;
    }

    static QQuickItem *find(QQuickItem *parent)
    {
        for (QQuickItem *child : parent->childItems()) {
            if (child->objectName() == QLatin1String("readingPage")) {
                return child;
            }
            if (QQuickItem *found = find(child)) {
                return found;
            }
        }
        return nullptr;
    }

    // One token per item from the focused one up, joined with `|` (`keys::classify`'s input).
    QString focusChain() const
    {
        QStringList tokens;
        for (QQuickItem *item = m_window->activeFocusItem(); item != nullptr; item = item->parentItem()) {
            if (item->inherits("QQuickPopupItem")) {
                tokens << QStringLiteral("popup");
            } else if (item->inherits("QQuickTextInput") || item->inherits("QQuickTextEdit")) {
                tokens << QStringLiteral("text");
            } else if (item->inherits("QQuickWebEngineView")) {
                tokens << QStringLiteral("web");
            } else if (item->inherits("QQuickControl")) {
                tokens << QStringLiteral("control");
            } else if (!item->objectName().isEmpty()) {
                tokens << item->objectName();
            } else {
                tokens << QStringLiteral("item");
            }
        }
        return tokens.join(QLatin1Char('|'));
    }

    QQuickWindow *m_window;
    QPointer<QQuickItem> m_page;
};

} // namespace

void ressac_init_webengine()
{
    // Before QtWebEngineQuick::initialize().
    QWebEngineUrlScheme scheme(QByteArrayLiteral("ressac"));
    scheme.setSyntax(QWebEngineUrlScheme::Syntax::Path);
    QWebEngineUrlScheme::registerScheme(scheme);
    QWebEngineUrlScheme mosaic(QByteArrayLiteral("ressac-mosaic"));
    mosaic.setSyntax(QWebEngineUrlScheme::Syntax::Path);
    QWebEngineUrlScheme::registerScheme(mosaic);
    QWebEngineUrlScheme toggle(QByteArrayLiteral("ressac-toggle"));
    toggle.setSyntax(QWebEngineUrlScheme::Syntax::Path);
    QWebEngineUrlScheme::registerScheme(toggle);

    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QtWebEngineQuick::initialize();
}

// Owned here so `ressac_destroy_application` can delete it before `main` returns: Qt and
// Chromium must not tear down during static destruction with threads still running.
static QApplication *g_application = nullptr;

void ressac_create_application()
{
    QCoreApplication::setOrganizationName(QStringLiteral("ressac"));
    QCoreApplication::setApplicationName(QStringLiteral("ressac-kde"));

    static int argc = 1;
    static char arg0[] = "ressac-kde";
    static char *argv[] = {arg0, nullptr};
    g_application = new QApplication(argc, argv);

    QQuickWebEngineProfile *profile = QQuickWebEngineProfile::defaultProfile();
    if (!profile->isOffTheRecord()) {
        qWarning("ressac-kde: the default web profile is not off the record");
    }
    profile->installUrlSchemeHandler("ressac", new RessacSchemeHandler(qApp));
    g_mosaic_handler = new MosaicHandler(qApp);
    profile->installUrlSchemeHandler("ressac-mosaic", g_mosaic_handler);
    profile->installUrlSchemeHandler("ressac-toggle", new ToggleHandler(qApp));

    QGuiApplication::setDesktopFileName(QStringLiteral("org.w3cdotorg.ressac"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.w3cdotorg.ressac")));
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("org.kde.desktop"));
    }
    // Outside a Plasma session nothing sets the icon theme.
    const QString theme = QIcon::themeName();
    if (theme.isEmpty() || theme == QLatin1String("hicolor")) {
        QIcon::setThemeName(QStringLiteral("breeze"));
    }
    QIcon::setFallbackThemeName(QStringLiteral("breeze"));

    // The bundled faces, from the scheme's own table, so the font menu and the preview draw in them.
    for (const char *file : {"Newsreader-Variable.ttf", "Newsreader-Italic-Variable.ttf",
                             "AtkinsonHyperlegibleNext-Variable.ttf",
                             "AtkinsonHyperlegibleNext-Italic-Variable.ttf", "bedstead.otf"}) {
        const std::string url = std::string("ressac:asset/") + file;
        rust::String mime;
        rust::Vec<std::uint8_t> body;
        if (!ressac_scheme_serve(rust::Str(url.data(), url.size()), mime, body)) {
            qWarning("ressac-kde: cannot read %s", file);
            continue;
        }
        const QByteArray data(reinterpret_cast<const char *>(body.data()), static_cast<qsizetype>(body.size()));
        const int id = QFontDatabase::addApplicationFontFromData(data);
        if (id == -1) {
            qWarning("ressac-kde: cannot register %s", file);
        } else if (std::string_view(file) == "bedstead.otf") {
            g_bedstead_family = QFontDatabase::applicationFontFamilies(id).value(0);
        }
    }
    if (g_bedstead_family.isEmpty()) {
        qWarning("ressac-kde: cannot register bedstead.otf");
    }
}

void ressac_mosaic_reply(std::uint64_t token, rust::Str mime, rust::Slice<const std::uint8_t> body)
{
    if (g_mosaic_handler.isNull()) {
        qCDebug(lcMosaic, "mosaic: dropped %llu", static_cast<unsigned long long>(token));
        return;
    }
    g_mosaic_handler->reply(token, mime, body);
}

rust::String ressac_bedstead_family()
{
    return to_rust(g_bedstead_family);
}

void ressac_frame_log_start()
{
    QQuickWindow *window = first_quick_window(true);
    if (window == nullptr) {
        return;
    }
    QObject::disconnect(g_frames.conn);
    {
        const std::lock_guard<std::mutex> guard(g_frames.lock);
        g_frames.ms.clear();
        g_frames.primed = false;
    }
    g_frames.conn = QObject::connect(
        window, &QQuickWindow::frameSwapped, window,
        [] {
            const std::lock_guard<std::mutex> guard(g_frames.lock);
            if (g_frames.primed) {
                g_frames.ms.push_back(static_cast<double>(g_frames.clock.nsecsElapsed()) / 1e6);
            }
            g_frames.primed = true;
            g_frames.clock.restart();
        },
        Qt::DirectConnection);
}

rust::String ressac_frame_log_stop()
{
    QObject::disconnect(g_frames.conn);
    std::vector<double> ms;
    {
        const std::lock_guard<std::mutex> guard(g_frames.lock);
        ms = g_frames.ms;
    }
    if (ms.empty()) {
        return rust::String("n=0");
    }
    std::sort(ms.begin(), ms.end());
    const std::size_t n = ms.size();
    double sum = 0;
    int over16 = 0;
    for (const double v : ms) {
        sum += v;
        if (v > 16.7) {
            ++over16;
        }
    }
    const QString line = QStringLiteral("n=%1 mean=%2 p50=%3 p95=%4 max=%5 over16=%6")
                             .arg(n)
                             .arg(sum / static_cast<double>(n), 0, 'f', 2)
                             .arg(ms[n / 2], 0, 'f', 2)
                             .arg(ms[n * 95 / 100], 0, 'f', 2)
                             .arg(ms.back(), 0, 'f', 2)
                             .arg(over16);
    return to_rust(line);
}

void ressac_frame_log_auto()
{
    if (qgetenv("RESSAC_FRAME_LOG") != "1") {
        return;
    }
    ressac_frame_log_start();
    auto *timer = new QTimer(qApp);
    QObject::connect(timer, &QTimer::timeout, timer, [] {
        rust::String line = ressac_frame_log_stop();
        qInfo("ressac-kde: frames %s", line.c_str());
        ressac_frame_log_start();
    });
    timer->start(10000);
}

void ressac_destroy_application()
{
    delete g_application;
    g_application = nullptr;
}

bool ressac_has_window()
{
    return first_quick_window(false) != nullptr;
}

int ressac_exec()
{
    return QCoreApplication::exec();
}

bool ressac_grab_window(rust::Str path)
{
    // The focused window (a Settings or Sea Chart window when one is up), else the first visible one.
    if (auto *focused = qobject_cast<QQuickWindow *>(QGuiApplication::focusWindow());
        focused != nullptr && focused->isVisible()) {
        return focused->grabWindow().save(from_rust(path));
    }
    QQuickWindow *window = first_quick_window(true);
    if (window == nullptr) {
        return false;
    }
    return window->grabWindow().save(from_rust(path));
}

rust::String ressac_clipboard_text()
{
    return to_rust(QGuiApplication::clipboard()->text());
}

void ressac_install_key_filter()
{
    static bool installed = false;
    QQuickWindow *window = first_quick_window(false);
    if (installed || window == nullptr) {
        return;
    }
    installed = true;
    window->installEventFilter(new ReadingKeyFilter(window, window));
}

bool ressac_send_key(std::int32_t key, std::uint32_t modifiers, rust::Str text)
{
    QQuickWindow *window = first_quick_window(true);
    if (window == nullptr) {
        return false;
    }
    const Qt::KeyboardModifiers mods(modifiers);
    const QString t = from_rust(text);
    QKeyEvent override(QEvent::ShortcutOverride, key, mods, t);
    override.ignore();
    QCoreApplication::sendEvent(window, &override);
    QKeyEvent press(QEvent::KeyPress, key, mods, t);
    const bool consumed = QCoreApplication::sendEvent(window, &press);
    QKeyEvent release(QEvent::KeyRelease, key, mods, t);
    QCoreApplication::sendEvent(window, &release);
    return consumed;
}

rust::String ressac_settings_default_file()
{
    return to_rust(QSettings(QStringLiteral("ressac"), QStringLiteral("ressac-kde")).fileName());
}

bool ressac_settings_get(rust::Str file, rust::Str key, rust::String &value)
{
    const QSettings settings = open(file);
    const QString k = from_rust(key);
    if (!settings.contains(k)) {
        return false;
    }
    value = to_rust(settings.value(k).toString());
    return true;
}

bool ressac_settings_set(rust::Str file, rust::Str key, rust::Str value)
{
    QSettings settings = open(file);
    settings.setValue(from_rust(key), from_rust(value));
    settings.sync();
    return settings.status() == QSettings::NoError;
}

bool ressac_settings_remove(rust::Str file, rust::Str key)
{
    QSettings settings = open(file);
    settings.remove(from_rust(key));
    settings.sync();
    return settings.status() == QSettings::NoError;
}

void ressac_draw_text_f(QPainter &painter, double x, double y, const QString &text)
{
    painter.drawText(QPointF(x, y), text);
}

double ressac_painter_scale_x(const QPainter &painter)
{
    return painter.deviceTransform().m11();
}

double ressac_painter_scale_y(const QPainter &painter)
{
    return painter.deviceTransform().m22();
}
