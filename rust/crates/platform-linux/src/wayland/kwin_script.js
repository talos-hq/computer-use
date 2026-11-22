// SPDX-License-Identifier: MIT
// Executed once by KWin. Only an explicitly supplied UUID/PID may be activated.
(function () {
    var windows = workspace.windowList();
    if (request.target !== null) {
        var matches = windows.filter(function (w) {
            return String(w.internalId) === request.target && w.pid === request.pid;
        });
        if (matches.length !== 1) {
            report({error: "KWin target UUID/PID no longer exists"});
            return;
        }
        matches[0].minimized = false;
        workspace.activeWindow = matches[0];
    }
    if (request.point) {
        var hits = workspace.windowAt({x: request.point.x, y: request.point.y}, 1);
        if (hits.length !== 1 || String(hits[0].internalId) !== request.point.uuid ||
                hits[0].pid !== request.point.pid || !hits[0].active) {
            report({error: "target_occluded: point does not hit the active KWin UUID/PID"});
            return;
        }
    }
    var stacking = workspace.stackingOrder;
    function visible(w) {
        return !w.deleted && !w.minimized &&
            (w.onAllDesktops || w.desktops.indexOf(workspace.currentDesktop) >= 0) &&
            (w.activities.length === 0 || w.activities.indexOf(workspace.currentActivity) >= 0);
    }
    function unoccluded(w) {
        var index = stacking.indexOf(w);
        if (index < 0) return false;
        var g = w.frameGeometry;
        return !stacking.some(function (other, position) {
            if (position <= index || other.desktopWindow || !visible(other)) return false;
            var r = other.frameGeometry;
            return r.width > 0 && r.height > 0 && r.x < g.x + g.width &&
                r.x + r.width > g.x && r.y < g.y + g.height && r.y + r.height > g.y;
        });
    }
    report({windows: windows.filter(function (w) {
        return w.normalWindow || w.dialog;
    }).map(function (w) {
        var g = w.frameGeometry;
        var c = w.clientGeometry;
        var output = w.output;
        return {
            uuid: String(w.internalId), pid: w.pid, title: w.caption,
            // Window's public scripting API exposes no native/X11 flag. Qt's
            // QObject wrapper identifies this known native class. Unknown
            // classes stay unattested; never infer native from DISPLAY/PID.
            native: String(w).indexOf("KWin::XdgToplevelWindow(") === 0,
            app_name: String(w.resourceClass),
            x: Math.round(g.x), y: Math.round(g.y),
            width: Math.round(g.width), height: Math.round(g.height),
            client_x: Math.round(c.x), client_y: Math.round(c.y),
            active: w.active, minimized: w.minimized,
            on_screen: visible(w), unoccluded: unoccluded(w),
            stacking: stacking.indexOf(w),
            desktops: w.desktops.map(function (d) { return String(d.id); }),
            output: output ? output.name : "", scale: output && output.devicePixelRatio ? output.devicePixelRatio : 1
        };
    })});
})();
