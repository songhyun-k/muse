"""Original, fictional music data and generated cover art for public previews."""
import json
from functools import cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


@cache
def load():
    source = json.loads((ROOT / 'backend/Fixtures/demo.json').read_text())
    tracks, albums = [], []
    album_ids = {}
    def reference(kind, index):
        return dict(source='library', kind=kind, id=f'fixture-{kind}-{index}')
    for index, song in enumerate(source['tracks']):
        key = (song['artist'], song['album'])
        album = album_ids.setdefault(key, len(album_ids))
        item = dict(song, ref=reference('song', index), albumRef=reference('album', album),
                    artistRef=reference('artist', album))
        if index == 0: item['artworkUrl'] = 'demo://cover'
        tracks.append(item)
        if album == len(albums):
            albums.append(dict(item, ref=reference('album', album), title=song['album']))
    corpus = dict(tracks=tracks, albums=albums, lyrics=source['lyrics'],
                  seed=dict(version=1, collections=[dict(id='evening', name=source['playlist']['name'], description='',
                            items=[tracks[i] for i in source['playlist']['tracks']])],
                            favorites=[tracks[i] for i in source['favorites']],
                            history=[tracks[i] for i in source['history']], lyrics={}),
                  player=dict(current=tracks[0], playing=True, position=12.0, updatedAt=1000.0,
                              queueCount=len(source['queue']), queueRevision=1, currentEntryId='playing',
                              repeatMode='off', shuffle=False, canSeek=True),
                  volume=dict(device='Demo', level=0.65, muted=False, canSetVolume=True, canMute=True),
                  queue=[dict(id=f'queue-{i}', item=tracks[n]) for i,n in enumerate(source['queue'])])
    # An original geometric cover. No downloaded image or music lyrics are embedded.
    colors = ((18, 24, 43), (225, 113, 144), (250, 211, 168))
    pixels = []
    for y in range(96):
        for x in range(96):
            ridge = abs(x - 47.5) + abs(y - 47.5) * 0.55
            a, b = (colors[1], colors[2]) if ridge < 26 else (colors[0], colors[1])
            blend = min(1, (y + x * 0.28) / 122)
            pixels.extend(round(left + (right - left) * blend) for left, right in zip(a, b))
    corpus['cover'] = pixels
    return corpus
