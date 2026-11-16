-- Expand the Lucide catalog without removing existing stored selections.
ALTER TABLE workflows DROP CONSTRAINT workflows_icon_check;
ALTER TABLE workflows ADD CONSTRAINT workflows_icon_check
 CHECK(icon IN ('workflow','git-branch','code','bug','shield-check','rocket','layers','list-todo','briefcase','lightbulb','flask-conical','git-pull-request','palette','infinity','clipboard-check','app-window','server','smartphone','database','compass','headset','swatch-book','pen-tool','mouse-pointer','accessibility','monitor','component','terminal','cloud','container','network','cpu','blocks','test-tube','circle-check','gauge','scan-search','lock-keyhole','target','presentation','calendar-days','users','handshake','megaphone','shopping-cart','graduation-cap','heart-pulse','banknote'));
