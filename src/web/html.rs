// ==================== 暗色主题通用样式 ====================
const DARK_CSS: &str = r#"
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'Segoe UI','Microsoft YaHei',sans-serif;background:#0a0e17;color:#e1e5ee;font-size:13px}
.header{background:linear-gradient(135deg,#1a1f2e,#0d1117);padding:12px 24px;display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid #2a3042}
.header h1{font-size:18px;color:#58a6ff}
.header .right{display:flex;align-items:center;gap:14px}
.header .status{display:flex;align-items:center;gap:8px}
.header .dot{width:10px;height:10px;border-radius:50%;background:#f85149;animation:pulse 2s infinite}
.dot.on{background:#3fb950}
@keyframes pulse{0%,100%{opacity:1}50%{opacity:.5}}
.logout-btn,.back-btn{padding:5px 14px;border:1px solid #30363d;border-radius:4px;background:transparent;color:#8b949e;cursor:pointer;font-size:12px;text-decoration:none}
.logout-btn:hover{color:#f85149;border-color:#f85149}
.back-btn:hover{color:#58a6ff;border-color:#58a6ff}

.summary{background:#161b22;border-bottom:1px solid #21262d;padding:8px 24px;display:flex;flex-wrap:wrap;gap:4px 18px;align-items:center;font-size:12px}
.summary .tag{padding:3px 10px;border-radius:4px;font-weight:700;font-size:12px}
.tg-blue{background:#0d419d55;color:#58a6ff;border:1px solid #1f6feb44}
.tg-green{background:#23863622;color:#3fb950;border:1px solid #3fb95044}
.tg-red{background:#f8514922;color:#f85149;border:1px solid #f8514944}
.tg-yellow{background:#d2992222;color:#d29922;border:1px solid #d2992244}
.summary .val{font-weight:700;color:#c9d1d9}
.green{color:#3fb950!important}.red{color:#f85149!important}.blue{color:#58a6ff!important}.yellow{color:#d29922!important}.grey{color:#484f58!important}

.ctrl-bar{background:#161b22;border-bottom:1px solid #21262d;padding:8px 24px;display:flex;gap:8px;flex-wrap:wrap}
.btn{padding:6px 18px;border:1px solid #30363d;border-radius:6px;background:#21262d;color:#c9d1d9;font-size:12px;font-weight:600;cursor:pointer;transition:all .15s}
.btn:hover{border-color:#58a6ff;color:#58a6ff}
.btn.active{background:#f85149;color:#fff;border-color:#f85149}
.btn-warn{background:#d2992244;color:#d29922;border-color:#d29922}
.btn-danger{background:#f8514933;color:#f85149;border-color:#f85149}

.main{padding:14px 24px;max-width:1600px;margin:0 auto}
.card{background:#161b22;border:1px solid #21262d;border-radius:8px;margin-bottom:14px}
.card-title{font-size:13px;color:#8b949e;text-transform:uppercase;letter-spacing:.8px;padding:10px 16px;border-bottom:1px solid #21262d;font-weight:700}
.card-body{padding:14px 16px}
.full{grid-column:1/-1}

table.dt{width:100%;border-collapse:collapse;font-size:12px}
table.dt th{background:#0d1117;color:#8b949e;font-weight:600;padding:8px 10px;text-align:left;border-bottom:1px solid #21262d;white-space:nowrap}
table.dt td{padding:7px 10px;border-bottom:1px solid #161b22;white-space:nowrap}
table.dt tr:hover td{background:#1c2333}
a.slink{color:#58a6ff;text-decoration:none;font-weight:600}a.slink:hover{text-decoration:underline}

.info-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(170px,1fr));gap:10px}
.info-box{padding:12px;background:#0d1117;border-radius:6px;border:1px solid #21262d}
.info-box .lbl{font-size:11px;color:#8b949e;margin-bottom:3px}
.info-box .val{font-size:20px;font-weight:bold;color:#58a6ff}

.tabs{display:flex;border-bottom:1px solid #21262d}.tab{padding:8px 20px;cursor:pointer;font-size:13px;color:#8b949e;border-bottom:2px solid transparent;transition:all .15s}.tab:hover{color:#58a6ff}.tab.active{color:#58a6ff;border-bottom-color:#58a6ff;font-weight:600}
.tc{display:none}.tc.active{display:block}

.log-box{height:420px;overflow-y:auto;font-family:Consolas,'Courier New',monospace;font-size:12px;background:#0d1117;border-radius:6px;padding:10px;border:1px solid #21262d}
.log-line{padding:2px 0;white-space:pre-wrap;word-break:break-all;line-height:1.5}
.log-time{color:#484f58}.lv-INFO{color:#3fb950}.lv-WARN{color:#d29922}.lv-ERROR{color:#f85149}.lv-DEBUG{color:#58a6ff}
.cm-green{color:#3fb950}.cm-red{color:#f85149}.cm-blue{color:#58a6ff}.cm-yellow{color:#d29922}.cm-cyan{color:#39d353}
.log-tb{display:flex;gap:8px;margin-bottom:8px;align-items:center}
.log-tb input{flex:1;padding:6px 12px;background:#0d1117;border:1px solid #30363d;border-radius:6px;color:#e1e5ee;font-size:12px;outline:none}
.log-tb input:focus{border-color:#58a6ff}

.row2{display:grid;grid-template-columns:1fr 1fr;gap:14px}
.chart-box{height:250px;position:relative}
::-webkit-scrollbar{width:6px}::-webkit-scrollbar-track{background:#0d1117}::-webkit-scrollbar-thumb{background:#30363d;border-radius:3px}
@media(max-width:900px){.row2{grid-template-columns:1fr}.info-grid{grid-template-columns:1fr 1fr}}
"#;

// ==================== JS 工具函数 ====================
const JS_UTILS: &str = r#"
let ws,flt='ALL',asc=true;
function $(id){return document.getElementById(id)}
function esc(s){if(!s)return'';const d=document.createElement('div');d.textContent=s;return d.innerHTML}
function f(v,d){d=d||2;return v==null?'0.00':Number(v).toFixed(d)}
function fc(v){v=Number(v);return v>0?'green':v<0?'red':''}
function getCookie(n){const v=document.cookie.match('(^|;)\\s*'+n+'=([^;]*)');return v?v[2]:null}
function sv(id,t){const e=$(id);if(e)e.textContent=t}
function conn(){
const p=location.protocol==='https:'?'wss:':'ws:';
ws=new WebSocket(p+'//'+location.host+'/ws?token='+(getCookie('crypto_token')||''));
ws.onopen=()=>{$('dot').classList.add('on');$('ws-st').textContent='已连接'};
ws.onclose=()=>{$('dot').classList.remove('on');$('ws-st').textContent='已断开';setTimeout(conn,3000)};
ws.onmessage=e=>{try{onMsg(JSON.parse(e.data))}catch(ex){}};
}
function tc(n){if(ws&&ws.readyState===1)ws.send(JSON.stringify({action:'control',control:n}))}
function upCtrl(c){
['b-fs','b-ss','b-os','b-fc'].forEach(id=>{const e=$(id);if(e)e.classList.remove('active')});
if(c.force_stop)$('b-fs').classList.add('active');if(c.soft_stop)$('b-ss').classList.add('active');
if(c.opening_stopped)$('b-os').classList.add('active');if(c.force_closing)$('b-fc').classList.add('active');
}
"#;

// ==================== 登录页 ====================

pub fn login_html(error: Option<&str>) -> String {
    let error_html = error.map(|e| {
        let escaped = e.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        format!(r#"<div class="err">{}</div>"#, escaped)
    }).unwrap_or_default();
    format!(r##"<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0"><title>Crypto - 登录</title>
<style>*{{margin:0;padding:0;box-sizing:border-box}}body{{font-family:'Segoe UI','Microsoft YaHei',sans-serif;background:#0a0e17;color:#e1e5ee;display:flex;justify-content:center;align-items:center;min-height:100vh}}.box{{background:#161b22;border:1px solid #21262d;border-radius:12px;padding:40px;width:380px;box-shadow:0 8px 32px rgba(0,0,0,.4)}}.box h1{{text-align:center;color:#58a6ff;font-size:24px;margin-bottom:8px}}.box .sub{{text-align:center;color:#484f58;font-size:13px;margin-bottom:30px}}.fg{{margin-bottom:20px}}.fg label{{display:block;color:#8b949e;font-size:13px;margin-bottom:6px}}.fg input{{width:100%;padding:10px 14px;background:#0d1117;border:1px solid #30363d;border-radius:6px;color:#e1e5ee;font-size:14px;outline:none}}.fg input:focus{{border-color:#58a6ff}}.lb{{width:100%;padding:12px;background:#238636;border:none;border-radius:6px;color:#fff;font-size:15px;font-weight:600;cursor:pointer}}.lb:hover{{background:#2ea043}}.err{{background:#f8514922;border:1px solid #f85149;color:#f85149;padding:10px;border-radius:6px;margin-bottom:15px;font-size:13px;text-align:center}}</style>
</head><body><div class="box"><h1>Crypto</h1><div class="sub">量化交易监控面板</div>{error_html}<form method="POST" action="/login"><div class="fg"><label>用户名</label><input type="text" name="username" required></div><div class="fg"><label>密码</label><input type="password" name="password" required></div><button type="submit" class="lb">登 录</button></form></div></body></html>"##)
}

// ==================== 主页: 策略列表 ====================

pub fn dashboard_html() -> String {
    format!(r##"<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0"><title>Crypto - 实盘管理</title>
<style>{css}</style></head><body>
<div class="header"><div style="display:flex;align-items:center;gap:12px"><h1>Crypto 实盘管理</h1><div class="dot" id="dot"></div><span id="ws-st" style="font-size:12px;color:#8b949e">连接中...</span></div><div class="right"><span style="color:#484f58" id="sn"></span><a href="/logout" class="logout-btn">退出</a></div></div>

<div class="summary">
<span class="tag tg-blue" id="tag-n">0台</span>
<span class="tag tg-blue">当前 <b id="s-bal">0</b></span>
<span class="tag tg-green">浮动 <b id="s-upnl">0</b></span>
<span class="tag tg-green">当日 <b id="s-today">0</b></span>
<span class="tag tg-yellow">总利润 <b id="s-total">0</b></span>
<span style="flex:1"></span>
<span>总: <span class="val" id="s-cnt">0</span></span>
<span>成功: <span class="val green" id="s-win">0</span></span>
<span>失败: <span class="val red" id="s-lose">0</span></span>
<span>胜率: <span class="val" id="s-wr">0%</span></span>
<span>盈利: <span class="val green" id="s-profit">0</span></span>
<span>亏损: <span class="val red" id="s-loss">0</span></span>
<span>交易量: <span class="val" id="s-vol">0</span></span>
</div>

<div class="ctrl-bar">
<button class="btn" id="b-ss" onclick="tc('soft_stop')">暂停交易</button>
<button class="btn" id="b-os" onclick="tc('opening_stopped')">停止开仓</button>
<button class="btn btn-warn" id="b-fc" onclick="tc('force_closing')">强制平仓</button>
<button class="btn btn-danger" id="b-fs" onclick="tc('force_stop')">账户强停</button>
</div>

<div class="main">
<div class="card"><div class="card-title">策略列表</div><div class="card-body" style="overflow-x:auto">
<table class="dt"><thead><tr><th></th><th>账号</th><th>交易对</th><th>交易所</th><th>订单类型</th><th>持仓</th><th>余额</th><th>总次数(胜率)</th><th>当日(胜率)</th><th>盈利</th><th>亏损</th><th>总盈亏</th><th>当日盈亏</th></tr></thead>
<tbody id="tb"><tr><td colspan="13" class="grey" style="text-align:center;padding:20px">暂无策略</td></tr></tbody>
</table></div></div>
</div>

<script>{js}
function onMsg(d){{
if(d.type==='stats')upMain(d.data);
else if(d.type==='controls')upCtrl(d.data);
}}
function upMain(s){{
if(!s)return;$('sn').textContent=s.server_name||'';
sv('s-bal',f(s.current_balance));sv('s-upnl',f(s.unrealized_pnl||0));
sv('s-today',f(s.today_profit||0));sv('s-total',f(s.total_profit));
sv('s-cnt',s.count||0);sv('s-win',s.success_count||0);sv('s-lose',s.failure_count||0);
sv('s-wr',((s.win_rate||0)*100).toFixed(1)+'%');
sv('s-profit',f(s.total_profit));sv('s-loss',f(s.total_loss||0));sv('s-vol',f(s.volume));
if(s.strategies)upStrats(s.strategies);
}}
function upStrats(list){{
const b=$('tb');if(!b||!Array.isArray(list))return;
$('tag-n').textContent=list.length+'台';
if(list.length===0){{b.innerHTML='<tr><td colspan="13" class="grey" style="text-align:center;padding:20px">暂无策略</td></tr>';return}}
b.innerHTML='';
list.forEach((s,i)=>{{
const n=s.name||s.symbol||('策略'+(i+1));const pnl=Number(s.total_profit||0);const tp=Number(s.today_profit||0);
const tr=document.createElement('tr');
tr.innerHTML='<td>'+(i+1)+'</td><td><a class="slink" href="/strategy/'+encodeURIComponent(n)+'">'+esc(n)+'</a></td><td>'+esc(s.symbol||'')+'</td><td>'+esc(s.exchange||'')+'</td><td>'+esc(s.order_type||'')+'</td><td class="'+(s.position_side==='Long'?'green':s.position_side==='Short'?'red':'')+'">'+f(s.position_amount||0,4)+'</td><td>'+f(s.balance)+'</td><td>'+(s.count||0)+' ('+((s.win_rate||0)*100).toFixed(1)+'%)</td><td>'+(s.today_count||0)+' ('+((s.today_win_rate||0)*100).toFixed(1)+'%)</td><td class="green">'+f(s.profit||0)+'</td><td class="red">'+f(s.loss||0)+'</td><td class="'+fc(pnl)+'"><b>'+f(pnl)+'</b></td><td class="'+fc(tp)+'"><b>'+f(tp)+'</b></td>';
b.appendChild(tr);
}});
}}
conn();
</script></body></html>"##, css=DARK_CSS, js=JS_UTILS)
}

// ==================== 详情页 ====================

pub fn detail_html(name: &str) -> String {
    let ne = name.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;");
    format!(r##"<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0"><title>{name} - Crypto</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
<style>{css}</style></head><body>
<div class="header"><div style="display:flex;align-items:center;gap:12px"><a href="/" class="back-btn">← 返回</a><h1>{name}</h1><div class="dot" id="dot"></div><span id="ws-st" style="font-size:12px;color:#8b949e">连接中...</span></div><div class="right"><span style="color:#484f58" id="sn"></span><a href="/logout" class="logout-btn">退出</a></div></div>

<div class="summary">
<span class="tag tg-blue">当前 <b id="s-bal">0</b></span>
<span class="tag tg-green">浮动 <b id="s-upnl">0</b></span>
<span class="tag tg-green">当日 <b id="s-today">0</b></span>
<span class="tag tg-yellow">总利润 <b id="s-total">0</b></span>
<span style="flex:1"></span>
<span>总: <span class="val" id="s-cnt">0</span></span> <span>成功: <span class="val green" id="s-win">0</span></span> <span>失败: <span class="val red" id="s-lose">0</span></span> <span>胜率: <span class="val" id="s-wr">0%</span></span> <span>交易量: <span class="val" id="s-vol">0</span></span>
</div>

<div class="ctrl-bar">
<button class="btn" id="b-ss" onclick="tc('soft_stop')">暂停交易</button>
<button class="btn" id="b-os" onclick="tc('opening_stopped')">停止开仓</button>
<button class="btn btn-warn" id="b-fc" onclick="tc('force_closing')">强制平仓</button>
<button class="btn btn-danger" id="b-fs" onclick="tc('force_stop')">账户强停</button>
</div>

<div class="main">
<!-- 账户信息 -->
<div class="card"><div class="card-title">账户信息</div><div class="card-body"><div class="info-grid">
<div class="info-box"><div class="lbl">总余额 (USDT)</div><div class="val" id="v-bal">0</div></div>
<div class="info-box"><div class="lbl">可用余额</div><div class="val" id="v-avail">0</div></div>
<div class="info-box"><div class="lbl">冻结保证金</div><div class="val" id="v-frozen">0</div></div>
<div class="info-box"><div class="lbl">未实现盈亏</div><div class="val" id="v-upnl">0</div></div>
<div class="info-box"><div class="lbl">初始余额</div><div class="val" id="v-init">0</div></div>
<div class="info-box"><div class="lbl">总收益率</div><div class="val" id="v-roi">0%</div></div>
<div class="info-box"><div class="lbl">今日利润</div><div class="val" id="v-tp">0</div></div>
<div class="info-box"><div class="lbl">资金费</div><div class="val" id="v-fund">0</div></div>
</div></div></div>

<!-- 持仓 -->
<div class="card"><div class="card-title">合仓详情</div><div class="card-body" style="overflow-x:auto">
<table class="dt"><thead><tr><th>ID</th><th>币种</th><th>方向</th><th>数量(价值)</th><th>开仓价格</th><th>标记价(vs开仓)</th><th>爆仓价</th><th>未实现盈亏</th><th>模式</th><th>杠杆</th></tr></thead>
<tbody id="pos-body"><tr><td colspan="10" class="grey" style="text-align:center;padding:16px">暂无持仓</td></tr></tbody>
</table></div></div>

<!-- 风控 + 订单 -->
<div class="row2">
<div class="card"><div class="card-title">风控统计</div><div class="card-body" style="overflow-x:auto">
<table class="dt"><thead><tr><th></th><th>总</th><th>成功</th><th>失败</th><th>胜率</th><th>盈利</th><th>亏损</th><th>总收益</th></tr></thead><tbody>
<tr><td>今日</td><td id="r1c">0</td><td class="green" id="r1w">0</td><td class="red" id="r1l">0</td><td id="r1r">0%</td><td class="green" id="r1p">0</td><td class="red" id="r1s">0</td><td id="r1t">0</td></tr>
<tr><td>总计</td><td id="r2c">0</td><td class="green" id="r2w">0</td><td class="red" id="r2l">0</td><td id="r2r">0%</td><td class="green" id="r2p">0</td><td class="red" id="r2s">0</td><td id="r2t">0</td></tr>
</tbody></table></div></div>
<div class="card"><div class="card-title">订单统计</div><div class="card-body" style="overflow-x:auto">
<table class="dt"><thead><tr><th></th><th>下单数</th><th>成交数</th><th>成交率</th><th>撤单数</th><th>撤单率</th><th>持仓时间</th></tr></thead><tbody>
<tr><td>总计</td><td id="o-t">0</td><td id="o-f">0</td><td id="o-fr">0%</td><td id="o-c">0</td><td id="o-cr">0%</td><td id="o-h">-</td></tr>
</tbody></table></div></div>
</div>

<!-- 盈利曲线 -->
<div class="card"><div class="card-title">盈利曲线</div><div class="card-body"><div class="chart-box"><canvas id="pnlChart"></canvas></div></div></div>

<!-- 自定义数据表 -->
<div class="card" id="tbl-sec" style="display:none"><div class="card-title">数据表格</div><div class="card-body" id="tbl-box" style="overflow-x:auto"></div></div>

<!-- 日志 -->
<div class="card">
<div class="tabs"><div class="tab active" onclick="stab('log',this)">运行日志</div><div class="tab" onclick="stab('trade',this)">交易记录</div><div class="tab" onclick="stab('fill',this)">成交记录</div></div>
<div class="card-body">
<div class="tc active" id="tc-log">
<div class="log-tb"><input type="text" id="lsrch" placeholder="搜索日志..." oninput="flog()">
<button class="btn" onclick="sf('ALL')">全部</button><button class="btn" onclick="sf('INFO')">INFO</button><button class="btn" onclick="sf('WARN')">WARN</button><button class="btn" onclick="sf('ERROR')">ERROR</button>
<button class="btn" onclick="xlog()" style="color:#58a6ff">导出</button><button class="btn" onclick="$('lc').innerHTML=''" style="color:#f85149">清空</button></div>
<div class="log-box" id="lc"></div>
</div>
<div class="tc" id="tc-trade"><table class="dt"><thead><tr><th>时间</th><th>币对</th><th>方向</th><th>类型</th><th>价格</th><th>数量</th><th>金额</th><th>盈亏</th><th>状态</th></tr></thead><tbody id="trade-b"><tr><td colspan="9" class="grey" style="text-align:center">暂无记录</td></tr></tbody></table></div>
<div class="tc" id="tc-fill"><table class="dt"><thead><tr><th>时间</th><th>币对</th><th>方向</th><th>成交价</th><th>成交量</th><th>手续费</th><th>订单ID</th></tr></thead><tbody id="fill-b"><tr><td colspan="7" class="grey" style="text-align:center">暂无记录</td></tr></tbody></table></div>
</div></div>

</div>

<script>{js}
let pnlChart=null,pnlD=[];
function onMsg(d){{
if(d.type==='log')addLog(d);else if(d.type==='stats')upS(d.data);else if(d.type==='controls')upCtrl(d.data);
else if(d.type==='positions')upPos(d.data);else if(d.type==='tables')upTbl(d.data);
}}
function addLog(l){{
const q=$('lsrch').value.toLowerCase();
if(flt!=='ALL'&&l.level!==flt)return;if(q&&l.msg.toLowerCase().indexOf(q)<0)return;
const c=$('lc'),d=document.createElement('div');d.className='log-line';d.setAttribute('data-level',l.level);d.setAttribute('data-msg',l.msg);
d.innerHTML='<span class="log-time">'+esc(l.time)+'</span> <span class="lv-'+esc(l.level)+'">['+esc(l.level)+']</span> <span class="cm-'+esc(l.color||'')+'">'+esc(l.msg)+'</span>';
c.appendChild(d);while(c.children.length>1000)c.removeChild(c.firstChild);if(asc)c.scrollTop=c.scrollHeight;
}}
function sf(l){{flt=l;flog()}}
function flog(){{const q=$('lsrch').value.toLowerCase();$('lc').querySelectorAll('.log-line').forEach(el=>{{const m=(el.getAttribute('data-msg')||'').toLowerCase();const lv=el.getAttribute('data-level')||'';el.style.display=(flt==='ALL'||lv===flt)&&(!q||m.indexOf(q)>=0)?'':'none'}})}}
function xlog(){{let t='';$('lc').querySelectorAll('.log-line').forEach(el=>{{t+=el.textContent+'\n'}});const a=document.createElement('a');a.href=URL.createObjectURL(new Blob([t],{{type:'text/plain'}}));a.download='logs.txt';a.click()}}

function upS(s){{
if(!s)return;$('sn').textContent=s.server_name||'';
sv('s-bal',f(s.current_balance));sv('s-upnl',f(s.unrealized_pnl||0));sv('s-today',f(s.today_profit||0));sv('s-total',f(s.total_profit));
sv('s-cnt',s.count||0);sv('s-win',s.success_count||0);sv('s-lose',s.failure_count||0);sv('s-wr',((s.win_rate||0)*100).toFixed(1)+'%');sv('s-vol',f(s.volume));
sv('v-bal',f(s.current_balance));sv('v-avail',f(s.available_balance||s.current_balance));sv('v-frozen',f(s.frozen_balance||0));
sv('v-upnl',f(s.unrealized_pnl||0));sv('v-init',f(s.initial_balance));sv('v-fund',f(s.funding_fee));sv('v-tp',f(s.today_profit||0));
const roi=s.initial_balance>0?((s.current_balance-s.initial_balance)/s.initial_balance*100):0;sv('v-roi',f(roi)+'%');
// colors
['v-upnl','v-tp'].forEach(id=>{{const e=$(id);if(e)e.className='val '+fc(Number(e.textContent))}});
const re=$('v-roi');if(re)re.className='val '+fc(roi);
// 风控
sv('r2c',s.count||0);sv('r2w',s.success_count||0);sv('r2l',s.failure_count||0);sv('r2r',((s.win_rate||0)*100).toFixed(1)+'%');sv('r2p',f(s.total_profit));sv('r2s',f(s.total_loss||0));sv('r2t',f(s.total_profit));
if(s.today){{sv('r1c',s.today.count||0);sv('r1w',s.today.success_count||0);sv('r1l',s.today.failure_count||0);sv('r1r',((s.today.win_rate||0)*100).toFixed(1)+'%');sv('r1p',f(s.today.profit||0));sv('r1s',f(s.today.loss||0));sv('r1t',f(s.today.total_profit||0))}}
// chart
if(s.current_balance>0){{const now=new Date();const lb=now.getHours()+':'+String(now.getMinutes()).padStart(2,'0');if(pnlD.length===0||pnlD[pnlD.length-1].l!==lb){{pnlD.push({{l:lb,p:Number(s.total_profit||0),b:Number(s.current_balance)}});if(pnlD.length>500)pnlD.shift();upChart()}}}}
}}

function upPos(pos){{
const b=$('pos-body');if(!b)return;
if(!pos||!Array.isArray(pos)||pos.length===0){{b.innerHTML='<tr><td colspan="10" class="grey" style="text-align:center;padding:16px">暂无持仓</td></tr>';return}}
b.innerHTML='';pos.forEach((p,i)=>{{const s=p.side||'';const sc=s==='Long'?'green':'red';const pnl=Number(p.unrealized_pnl||0);const pc=fc(pnl);const e=Number(p.entry_price||0);const m=Number(p.mark_price||0);const vs=e>0?((m-e)/e*100).toFixed(2)+'%':'';const a=Number(p.amount||0);
const tr=document.createElement('tr');tr.innerHTML='<td>'+(i+1)+'</td><td><b>'+esc(p.symbol)+'</b></td><td class="'+sc+'"><b>'+s+'</b></td><td>'+f(a,4)+' ($'+f(a*m,0)+')</td><td>'+f(e,1)+'</td><td>'+f(m,1)+' <span class="'+pc+'">'+vs+'</span></td><td>'+(p.liquidation_price?f(p.liquidation_price,1):'-')+'</td><td class="'+pc+'"><b>'+f(pnl)+'</b></td><td>'+(p.margin_mode||'cross')+'</td><td>'+(p.leverage||'-')+'x</td>';
b.appendChild(tr)}});
}}

function upTbl(t){{
if(!t||!t.length){{$('tbl-sec').style.display='none';return}}$('tbl-sec').style.display='';const b=$('tbl-box');b.innerHTML='';
t.forEach(tb=>{{if(tb.title){{const h=document.createElement('div');h.style.cssText='color:#8b949e;font-weight:700;margin:8px 0 4px';h.textContent=tb.title;b.appendChild(h)}}const tl=document.createElement('table');tl.className='dt';if(tb.cols){{const th=document.createElement('thead'),tr=document.createElement('tr');tb.cols.forEach(c=>{{const td=document.createElement('th');td.textContent=c;tr.appendChild(td)}});th.appendChild(tr);tl.appendChild(th)}}if(tb.rows){{const tb2=document.createElement('tbody');tb.rows.forEach(r=>{{const tr=document.createElement('tr');(Array.isArray(r)?r:[r]).forEach(c=>{{const td=document.createElement('td');td.textContent=c;tr.appendChild(td)}});tb2.appendChild(tr)}});tl.appendChild(tb2)}}b.appendChild(tl)}})
}}

function stab(n,el){{document.querySelectorAll('.tab').forEach(t=>t.classList.remove('active'));el.classList.add('active');document.querySelectorAll('.tc').forEach(c=>c.classList.remove('active'));$('tc-'+n).classList.add('active')}}

function upChart(){{
if(!pnlChart){{const ctx=$('pnlChart').getContext('2d');pnlChart=new Chart(ctx,{{type:'line',data:{{labels:[],datasets:[{{label:'盈利',data:[],borderColor:'#58a6ff',backgroundColor:'rgba(88,166,255,0.1)',fill:true,tension:.3,pointRadius:0,borderWidth:1.5}},{{label:'余额',data:[],borderColor:'#3fb950',backgroundColor:'transparent',fill:false,tension:.3,pointRadius:0,borderWidth:1,yAxisID:'y1'}}]}},options:{{responsive:true,maintainAspectRatio:false,interaction:{{intersect:false,mode:'index'}},plugins:{{legend:{{position:'top',labels:{{color:'#8b949e',font:{{size:11}},usePointStyle:true}}}}}},scales:{{x:{{ticks:{{color:'#484f58',font:{{size:10}},maxTicksLimit:20}},grid:{{color:'#21262d'}}}},y:{{position:'left',ticks:{{color:'#484f58',font:{{size:10}}}},grid:{{color:'#21262d'}}}},y1:{{position:'right',grid:{{drawOnChartArea:false}},ticks:{{color:'#484f58',font:{{size:10}}}}}}}}}}}})}}
pnlChart.data.labels=pnlD.map(d=>d.l);pnlChart.data.datasets[0].data=pnlD.map(d=>d.p);pnlChart.data.datasets[1].data=pnlD.map(d=>d.b);pnlChart.update('none');
}}

$('lc').addEventListener('scroll',function(){{asc=this.scrollTop+this.clientHeight>=this.scrollHeight-50}});
conn();
</script></body></html>"##, name=ne, css=DARK_CSS, js=JS_UTILS)
}
