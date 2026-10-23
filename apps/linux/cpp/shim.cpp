#include "shim.h"

#include <QtCore/QByteArray>
#include <QtCore/QCoreApplication>
#include <QtCore/QSettings>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>
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

} // namespace

void ressac_init_webengine()
{
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QtWebEngineQuick::initialize();
}

void ressac_create_application()
{
    QCoreApplication::setOrganizationName(QStringLiteral("ressac"));
    QCoreApplication::setApplicationName(QStringLiteral("ressac-kde"));

    static int argc = 1;
    static char arg0[] = "ressac-kde";
    static char *argv[] = {arg0, nullptr};
    new QApplication(argc, argv);

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
