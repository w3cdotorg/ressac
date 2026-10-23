#include "shim.h"
#include <QtCore/QCoreApplication>
#include <QtCore/QTimer>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>
#include <QtQuick/QQuickWindow>
#include <QtWebEngineQuick/QtWebEngineQuick>
#include <cstdio>

void ressac_init_webengine()
{
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QtWebEngineQuick::initialize();
}

void ressac_schedule_grab(int ms, rust::Str path)
{
    const QString p = QString::fromUtf8(path.data(), static_cast<qsizetype>(path.size()));
    QTimer::singleShot(ms, [p]() {
        for (QWindow *w : QGuiApplication::topLevelWindows()) {
            if (auto *qw = qobject_cast<QQuickWindow *>(w)) {
                const QImage img = qw->grabWindow();
                const bool ok = img.save(p);
                std::fprintf(stderr, "[spike] grab %s -> %dx%d saved=%d\n",
                             qPrintable(p), img.width(), img.height(), ok);
                return;
            }
        }
        std::fprintf(stderr, "[spike] grab: no QQuickWindow\n");
    });
}

void ressac_schedule_quit(int ms)
{
    QTimer::singleShot(ms, []() { QCoreApplication::quit(); });
}

#include <QtGui/QIcon>
#include <QtWidgets/QApplication>

void ressac_create_qapplication()
{
    static int argc = 1;
    static char arg0[] = "ressac-qt-spike";
    static char *argv[] = {arg0, nullptr};
    new QApplication(argc, argv);
}

void ressac_setup_icons()
{
    QIcon::setThemeName(QStringLiteral("breeze"));
    QIcon::setFallbackThemeName(QStringLiteral("breeze"));
    std::fprintf(stderr, "[spike] app class=%s style=%s icon theme=%s\n",
                 QCoreApplication::instance()->metaObject()->className(),
                 qPrintable(qEnvironmentVariable("QT_QUICK_CONTROLS_STYLE")),
                 qPrintable(QIcon::themeName()));
}

int ressac_exec()
{
    return QCoreApplication::exec();
}
