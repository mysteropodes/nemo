// Presentation adapter for project Open/Resume after canvas resize.
(function(){
  function repaintProjectEntry(){
    // Native Open/Resume can restore live paths while the canvas stays blank
    // until an explicit render. Cross one complete presentation boundary
    // after hiding the start screen: the canvas ResizeObserver can resize
    // (and clear) the WebGPU surface after callbacks in that first frame.
    // Painting in the following frame keeps the imported scene visible while
    // leaving the viewport and stored document exactly as the import left them.
    requestAnimationFrame(function(){
      requestAnimationFrame(function(){
        if(window.SMEngineBridge&&SMEngineBridge.isEnabled())SMEngineBridge.renderNow();
        else if(typeof view!=='undefined'&&view)view.update();
      });
    });
  }
  window.SMProjectEntry={repaint:repaintProjectEntry};
})();
