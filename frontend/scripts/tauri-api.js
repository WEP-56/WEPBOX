function resolveTauriWindow(){
  if(!windowApi) return null;
  if(typeof windowApi.getCurrentWindow === 'function') return windowApi.getCurrentWindow();
  if(typeof windowApi.getCurrent === 'function') return windowApi.getCurrent();
  if(windowApi.appWindow) return windowApi.appWindow;
  if(windowApi.WebviewWindow && typeof windowApi.WebviewWindow.getCurrent === 'function') return windowApi.WebviewWindow.getCurrent();
  return null;
}

const tauriWindow = resolveTauriWindow();

async function minimizeWindow(){
  try{ await tauriWindow?.minimize?.(); }catch(err){ showToast(formatError(err)); }
}

async function toggleMaximizeWindow(){
  if(!tauriWindow) return;
  try{
    if(typeof tauriWindow.toggleMaximize === 'function'){
      await tauriWindow.toggleMaximize();
      return;
    }
    const maximized = await tauriWindow.isMaximized();
    if(maximized) {
      await tauriWindow.unmaximize();
    } else {
      await tauriWindow.maximize();
    }
  }catch(err){
    showToast(formatError(err));
  }
}

async function closeWindow(){
  try{ await tauriWindow?.close?.(); }catch(err){ showToast(formatError(err)); }
}

async function enterBackgroundMode(){
  try{
    if(invoke){
      await invoke('enter_background_mode');
      return;
    }
    await tauriWindow?.hide?.();
  }catch(err){
    showToast(formatError(err));
  }
}
