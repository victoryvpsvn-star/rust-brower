// Bảng phím tắt dùng chung cho UI chính và mọi trang web.
(function (root) {
  root.__aurora_keys = function (send) {
    addEventListener('keydown', function (e) {
      var c = e.ctrlKey || e.metaKey;
      var k = e.key;
      var a = null;
      if (c && !e.altKey) {
        var l = k.toLowerCase();
        if (l === 't') a = e.shiftKey ? 'reopen' : 'newtab';
        else if (l === 'w') a = 'closetab';
        else if (l === 'l' || l === 'k') a = 'focusurl';
        else if (l === 'r') a = 'reload';
        else if (l === 'f') a = 'find';
        else if (l === 'd') a = 'bookmark';
        else if (l === 'b') a = 'sidebar';
        else if (k === '=' || k === '+') a = 'zoomin';
        else if (k === '-') a = 'zoomout';
        else if (k === '0') a = 'zoomreset';
        else if (k === 'Tab') a = e.shiftKey ? 'prevtab' : 'nexttab';
        else if (/^[1-9]$/.test(k)) a = 'tab' + k;
      } else if (k === 'F5') a = 'reload';
      else if (k === 'F12') a = 'devtools';
      else if (e.altKey && k === 'ArrowLeft') a = 'back';
      else if (e.altKey && k === 'ArrowRight') a = 'fwd';
      if (a) {
        e.preventDefault();
        e.stopPropagation();
        send('act|' + a);
      }
    }, true);
  };
})(window);
