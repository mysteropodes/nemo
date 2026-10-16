/* Existing Labs behavior behind explicit caller-owned ports. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoLabsSettingsUI=api;}(typeof globalThis==='object'?globalThis:this,function(){
  function create(ports){
  var document=ports.document,TOOLS=ports.tools,ACTIONS=ports.actions;
  function describeOf(name) {
    if (!ports.canList()) return '';
    var found = ports.list().find(function (p) { return p.name === name; });
    return found ? found.what : '';
  }

  function renderLabsPanel() {
    if (!ports.ready()) return;
    var toolsEl = document.getElementById('labs-tools-list');
    var actionsEl = document.getElementById('labs-actions-list');
    if (!toolsEl || !actionsEl) return;
    toolsEl.innerHTML = '';
    actionsEl.innerHTML = '';

    TOOLS.forEach(function (spec) {
      if (!ports.canIsOn()) return;
      var row = ports.mkRow();
      var head = ports.mkHeadLine();
      var on = ports.isOn(spec.name);
      var cb = ports.mkCheckbox(on, function (checked) {
        ports.setOn(spec.name,checked);
        renderLabsPanel(); // rebuild so inline params show/hide with the toggle
        ports.refreshFloating(); // same toggle, other entry point
      });
      head.appendChild(cb);
      head.appendChild(ports.mkLabel(ports.translate(spec.label), describeOf(spec.name) || (spec.hintExtra && ports.translate(spec.hintExtra))));
      row.appendChild(head);
      if (on && spec.extra) spec.extra(row);
      if (on && spec.hintExtra && !spec.extra) {
        var hint = ports.subRow();
        var hs = document.createElement('span'); hs.style.cssText = 'font-size:9px;color:var(--text-dim);'; hs.textContent = ports.translate(spec.hintExtra);
        hint.appendChild(hs); row.appendChild(hint);
      }
      toolsEl.appendChild(row);
    });

    ACTIONS.forEach(function (spec) {
      var row = ports.mkRow();
      var lbl = document.createElement('div');
      lbl.style.cssText = 'font-size:11px;color:var(--text);margin-bottom:3px;';
      lbl.textContent = ports.translate(spec.label);
      lbl.title = describeOf(spec.name);
      row.appendChild(lbl);
      var controls = ports.subRow();
      controls.style.paddingLeft = '0';
      spec.build(controls);
      row.appendChild(controls);
      actionsEl.appendChild(row);
    });
  }

  return Object.freeze({render:renderLabsPanel});
  }
  return Object.freeze({create:create});
}));
