#include "shim.h"

#include "ressac-kde/src/lib.cxx.h"

#include <QtCore/QBuffer>
#include <QtCore/QByteArray>
#include <QtCore/QCoreApplication>
#include <QtCore/QMetaObject>
#include <QtCore/QPointer>
#include <QtCore/QSettings>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QVariant>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QKeyEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>
#include <QtWebEngineCore/QWebEngineUrlRequestJob>
#include <QtWebEngineCore/QWebEngineUrlScheme>
#include <QtWebEngineCore/QWebEngineUrlSchemeHandler>
#include <QtWebEngineQuick/QQuickWebEngineProfile>
#include <QtWebEngineQuick/QtWebEngineQuick>
#include <QtWidgets/QApplication>

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
            } else if (item->inherits("QQuickAbstractButton")) {
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
    QQuickWindow *window = first_quick_window(true);
    if (window == nullptr) {
        return false;
    }
    return window->grabWindow().save(from_rust(path));
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
