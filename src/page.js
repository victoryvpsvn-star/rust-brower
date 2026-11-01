// Chèn vào mọi trang web: favicon, link mở tab mới, đóng gợi ý khi bấm vào trang.
(function () {
  var send = function (m) { try { window.ipc.postMessage(m); } catch (_) {} };
  if (window.__aurora_keys) window.__aurora_keys(send);

  var abs = function (u) { try { return new URL(u, location.href).href; } catch (_) { return null; } };
  window.open = function (u) { var a = u && abs(u); if (a) send('open|' + a); return null; };

  document.addEventListener('click', function (e) {
    var a = e.target && e.target.closest && e.target.closest('a[target=_blank]');
    if (a && a.href) { e.preventDefault(); send('open|' + a.href); }
  }, true);

  addEventListener('mousedown', function () { send('act|pageclick'); }, true);

  if (window.top === window) {
    var icon = function () {
      if (!/^https?:$/.test(location.protocol)) return;
      var l = document.querySelector('link[rel~="icon" i]') ||
              document.querySelector('link[rel="shortcut icon" i]') ||
              document.querySelector('link[rel~="apple-touch-icon" i]');
      send('icon|' + (l && l.href ? l.href : location.origin + '/favicon.ico'));
    };
    addEventListener('DOMContentLoaded', icon);
    addEventListener('load', icon);
  }
})();
