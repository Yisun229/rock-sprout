; 卸载钩子：只删 WebView2 的缓存目录，别的一概不碰。
;
; 两条红线：
;   1) %LOCALAPPDATA%\RockSprout\data 是用户数据，卸载必须留着 —— 这个文件里
;      从头到尾没有对它的任何 RMDir，也不许加。
;   2) 删不掉就跳过：文件被占、没权限都不弹窗、不中断卸载。RMDir /r 失败只是
;      返回错误码，不会弹框；不写 MessageBox，也不清错误标志。
;
; 路径写死，不用通配符、不扫同级目录：
;   %LOCALAPPDATA%\com.rocksprout.client  —— 当前标识符的 WebView2 缓存（约 80 MB）
;   %LOCALAPPDATA%\com.rocksprout.app     —— 旧标识符时期留下的缓存（约 10 MB）

!macro NSIS_HOOK_POSTUNINSTALL
  DetailPrint "清理浏览器缓存目录"
  RMDir /r "$LOCALAPPDATA\com.rocksprout.client"
  RMDir /r "$LOCALAPPDATA\com.rocksprout.app"
!macroend
