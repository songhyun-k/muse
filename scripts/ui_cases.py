"""Deterministic inputs for the production renderer, containing only fictional data."""
import copy
from demo_fixture import load

PAGES = ['검색', '홈', '최근 추가', '아티스트', '앨범', '노래', '플레이리스트', '즐겨찾기', '재생 이력', '상세', '가사']


def case(name, language='en', theme=1, page='노래', width=140, height=40, **overrides):
    data = load()
    p = dict(page=page, return_page='앨범', focus='main', preferred_panel='nav', panel='lyrics',
             cursors=dict(main=0, right=0), nav_cursor=5, left_open=True, right_open=True,
             style=theme, transparent=False, plain_icons=True, motion=False, language=language,
             query='', help=False, lyric_manual=None, hover=None, hover_time=0.0, pulse_key=None,
             pulse_time=-100.0, favorite_pulse=[-1, -100.0], message='', message_time=-100.0,
             editing=None, buffer='', now=1.0, art_time=1.0, energy=1.0, position_draw=36.0,
             position=36.0, current=0, volume_draw=65.0, nav_position=5.0,
             selections=dict(main=0.0, right=0.0), lyric_scroll=1.0, lyric_emphasis=2.0,
             panel_sizes=None)
    p.update(overrides)
    if page == '가사':
        p.update(left_open=False, right_open=False)
    tracks, albums = data['tracks'], data['albums']
    player = copy.deepcopy(data['player'])
    player.update(position=36.0, updatedAt=1000.0)
    saved = data['seed']
    store = dict(collections=[dict(id=c['id'], name=c['name'], description=c['description'], count=len(c['items'])) for c in saved['collections']],
                 favorites=[i['ref'] for i in saved['favorites']], historyCount=len(saved['history']))
    notices = [('snapshot', dict(session=dict(authorization='authorized', canPlayCatalog=True),
                                 player=player, store=store, volume=data['volume']))]
    if page in ('홈', '최근 추가', '앨범'):
        rows = albums
    elif page == '아티스트':
        rows = [dict(a, ref=a['artistRef'], title=a['artist']) for a in albums]
    else:
        rows = tracks
    if page in ('상세', '플레이리스트'):
        item = albums[1] if page == '상세' else dict(ref=dict(source='collection',kind='playlist',id='evening'),title='Evening',artist='',album='')
        notices.append(('detail', dict(item=item, children=rows)))
    else:
        notices.append(('page', dict(items=rows)))
    notices += [('queue', dict(entries=data['queue'], revision=1, total=len(data['queue']))),
                ('lyrics', dict(item=tracks[0]['ref'], status='synced', offset=0,
                                lines=[dict(seconds=i*18.0, text=line) for i,line in enumerate(data['lyrics'])]))]
    events = [dict(version=1,sequence=i+1,event=dict(type=kind,data=body)) for i,(kind,body) in enumerate(notices)]
    return dict(name=name,width=width,height=height,presentation=p,events=events,albums=albums,tracks=tracks)
