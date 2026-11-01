// Ad Block Lite - chèn vào mọi trang khi bật trong Cài đặt.
// Chặn ở mức DOM: ẩn iframe/ảnh/script quảng cáo, gỡ khối quảng cáo phổ biến
// và đếm số phần tử đã chặn để báo về Aurora (hiện trong Cài đặt).
(function () {
  if (window.top !== window) return; // chỉ chạy ở khung chính
  var send = function (m) { try { window.ipc.postMessage(m); } catch (_) {} };

  // Danh sách host quảng cáo / theo dõi phổ biến (gọn, lấy ý từ EasyList).
  var HOSTS = [
    'doubleclick.net', 'googlesyndication.com', 'googletagservices.com',
    'googleadservices.com', 'google-analytics.com', 'adservice.google.com',
    'adsystem.amazon.com', 'adnxs.com', 'adsrvr.org', 'criteo.com', 'criteo.net',
    'taboola.com', 'outbrain.com', 'outbrainimg.com', 'pubmatic.com',
    'rubiconproject.com', 'openx.net', 'smartadserver.com', 'casalemedia.com',
    'moatads.com', 'adsafeprotected.com', 'scorecardresearch.com',
    'quantserve.com', 'adform.net', 'adroll.com', 'yieldmo.com', '33across.com',
    'sharethrough.com', 'media.net', 'mediavine.com', 'adthrive.com',
    'popads.net', 'popcash.net', 'adcash.com', 'propellerads.com',
    'exoclick.com', 'juicyads.com', 'trafficjunky.net', 'mgid.com',
    'revcontent.com', 'adskeeper.com', 'bidgear.com', '-onetag.com'
  ];
  var isAd = function (u) {
    if (!u) return false;
    var s = ('' + u).toLowerCase();
    for (var i = 0; i < HOSTS.length; i++) {
      if (s.indexOf(HOSTS[i]) !== -1) return true;
    }
    return false;
  };

  var SEL = [
    'ins.adsbygoogle', 'div[id^="google_ads"]', 'div[id^="div-gpt-ad"]',
    'div[id^="taboola-"]', 'div[class*="OUTBRAIN"]', 'iframe[id^="google_ads_frame"]',
    'div[data-ad]', 'div[class^="ad-slot"]', 'div[id^="ad_position"]',
    'a[href*="doubleclick.net"]', 'div[class*="banner-ad"]',
    'iframe[src*="googlesyndication.com"]', 'div[class^="dfp-ad"]',
    'amp-ad', 'div[data-ad-manager-id]'
  ].join(',');

  var n = 0;
  var hide = function (el) {
    if (!el || el.__aurora_ad) return;
    el.__aurora_ad = 1;
    if (el.style) { el.style.display = 'none'; }
    n++;
  };

  var scan = function () {
    try {
      // ẩn theo selector
      var list = document.querySelectorAll(SEL);
      for (var i = 0; i < list.length; i++) hide(list[i]);
      // ẩn iframe/ảnh/video quảng cáo theo host
      var fr = document.querySelectorAll('iframe, img, video, source');
      for (var j = 0; j < fr.length; j++) {
        var src = fr[j].src || fr[j].data_src || '';
        if (isAd(src)) hide(fr[j]);
      }
    } catch (_) {}
  };

  var report = function () {
    if (n > 0) { send('adblock|' + n); n = 0; }
  };

  var start = function () {
    scan();
    report();
    // theo dõi DOM thay đổi (trang lazy-load quảng cáo liên tục)
    try {
      var mo = new MutationObserver(function () { scan(); });
      mo.observe(document.documentElement, { childList: true, subtree: true });
      // gom báo cáo theo lô mỗi 2 giây để không làm chậm trang
      setInterval(report, 2000);
    } catch (_) {}
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start, { once: true });
  } else {
    start();
  }
})();
