// Reader formatting for received message bodies: plain text becomes paragraph
// HTML with links, and a body is split into new text and quoted reply history.
#pragma once
#include <QFont>
#include <QFontMetricsF>
#include <QRegularExpression>
#include <QSet>
#include <QTextDocument>
#include <QUrl>
#include <QVariantList>
#include <functional>
#include <algorithm>

// Display-only projection. Source bytes and authored/composer text never change.
// Header-only; the functions are pure apart from building QTextDocuments, so
// they run on the GUI thread or MailBackend's reader workers.
namespace ReceivedBody {
/// The QTextDocument proportional line height, in percent, that matches the
/// design's CSS line-height of 1.65 for the Geist font. Computed once.
///
/// Qt's proportional line height scales the font's natural line height; the
/// design's plain-text reader uses CSS line-height 1.65 (a font-size multiple).
inline int readerLeadingPercent() {
    static const int percent = [] {
        QFont font(QStringLiteral("Geist")); font.setPixelSize(100);
        const qreal natural = QFontMetricsF(font).height() / 100.0;
        return qRound(1.65 / (natural > 0.5 ? natural : 1.0) * 100);
    }();
    return percent;
}
/// Converts plain text to reader HTML: one `<p>` per blank-line-separated
/// paragraph, single newlines as `<br>`, repeated spaces kept, text escaped,
/// and strictly valid `http(s)://` URLs turned into `<a href>` links.
/// Paragraphs are `gap` pixels apart, and the last one keeps `last` below it
/// (the same as `gap` when negative).
inline QString plainHtml(QString text, int gap = 24, int last = -1) {
    text.replace("\r\n","\n"); text.replace('\r','\n');
    const auto paragraphs=text.trimmed().split(QRegularExpression("\\n[\\t ]*\\n(?:[\\t ]*\\n)*"));
    QString html; qsizetype index=0;
    for(const auto &p:paragraphs) {
        const int below=++index==paragraphs.size() && last>=0 ? last : gap;
        QString rendered;qsizetype end=0;
        auto links=QRegularExpression("https?://[^\\s<>]+",QRegularExpression::CaseInsensitiveOption).globalMatch(p);
        while(links.hasNext()) {
            const auto link=links.next();rendered+=p.mid(end,link.capturedStart()-end).toHtmlEscaped();
            const auto escaped=link.captured().toHtmlEscaped();const QUrl url(link.captured(),QUrl::StrictMode);
            rendered+=url.isValid()?"<a href=\""+escaped+"\">"+escaped+"</a>":escaped;end=link.capturedEnd();
        }
        rendered+=p.mid(end).toHtmlEscaped();
        // A blank line between paragraphs keeps one reader line of space.
        html += "<p style=\"white-space:pre-wrap;margin-top:0;margin-bottom:"+QString::number(below)+"px;line-height:"+QString::number(readerLeadingPercent())+"%\">"+rendered.replace('\n',"<br>")+"</p>";
    }
    return html;
}
/// Whether a trimmed line opens a forwarded message: Apple Mail's "Begin
/// forwarded message:" or a "---------- Forwarded message ---------" rule.
inline bool forwardMarker(const QString &line) {
    static const QRegularExpression marker("^[-_=\\s]*(?:begin forwarded message|forwarded message)[-_=\\s:]*$",QRegularExpression::CaseInsensitiveOption);
    return marker.match(line).hasMatch();
}
/// How many `>` quote marks open `line`, and the text after them. Indentation
/// of an unquoted line is kept.
inline std::pair<int,QString> quoteLevel(const QString &line) {
    int level=0; qsizetype i=0;
    for(;;) {
        qsizetype j=i; while(j<line.size() && line[j]==' ') ++j;
        if(j>=line.size() || line[j]!='>') break;
        ++level; i=j+1; if(i<line.size() && line[i]==' ') ++i;
    }
    return {level,line.mid(i)};
}
/// Whether `text` is a reply attribution such as "On Tuesday, Alex wrote:",
/// in English or another common client language.
inline bool attribution(const QString &text) {
    static const QRegularExpression ending("(?:wrote|a \\x{e9}crit|schrieb|escribi\\x{f3}|scrisse|escreveu|schreef|skrev)\\s*:$",QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression digit("\\d");
    return text.size()<=400 && ending.match(text).hasMatch() && (text.startsWith("On ") || digit.match(text).hasMatch());
}
/// Splits a received body into the sender's new text, dropping quoted reply
/// history. Uses `html` when non-empty, else `plain`.
///
/// History is recognised by `>` lines, "On ... wrote:" attributions,
/// "Original Message" separators, Outlook-style From/To/Date/Subject header
/// blocks, `<blockquote>`, Gmail, Yahoo and Proton quote containers, and
/// Outlook's `divRplyFwdMsg`. Inline answers between quoted lines are kept,
/// and a forwarded message is kept whole. `sanitize` turns a rich HTML
/// fragment into display HTML; plain fragments go through plainHtml().
///
/// A plain forward is laid out in rows the reader styles by `kind`:
/// - `forward`: the "Forwarded message" label;
/// - `headers`: its From/Date/To/Subject block;
/// - no kind: the forwarded text;
/// - `attribution`: an "On … wrote:" line;
/// - `quote`: history the forward quotes.
/// `depth` counts quote levels below the forwarded text.
///
/// Returns rows of {html, parents, quote}, plus `kind` and `depth` for a
/// forward's rows. Only new-text rows are emitted, so `parents` is always
/// empty and `quote` false. Rows whose visible text is blank are omitted.
/// Nesting is followed at most 32 levels deep.
inline QVariantList sections(const QString &html,const QString &plain,const std::function<QString(const QString &)> &sanitize) {
    QVariantList rows; int serial=0;
    auto row=[&](const QString &rendered,const QString &kind,int depth) {
        // Markup-only fragments (empty divs, spacer tables) have no visible text.
        QTextDocument check; check.setHtml(rendered);
        if(check.toPlainText().trimmed().isEmpty())return;
        QVariantMap entry{{"html",rendered},{"parents",QVariantList{}},{"quote",false}};
        if(!kind.isEmpty()){entry["kind"]=kind;entry["depth"]=depth;}
        rows.append(entry);
    };
    auto append=[&](const QString &body,const QVariantList &parents,bool rich) {
        if(!parents.isEmpty() || body.trimmed().isEmpty())return;
        row(rich?sanitize(body):plainHtml(body),{},0);
    };
    // A forward from its marker line on: the label, the forwarded message's
    // header block and text, and the history it quotes, one row per level.
    std::function<void(const QStringList &,int)> forward;
    forward=[&](const QStringList &lines,int depth) {
        row(QStringLiteral("Forwarded message"),QStringLiteral("forward"),0);
        int first=1;
        while(first<lines.size() && quoteLevel(lines[first]).second.trimmed().isEmpty()) ++first;
        // Clients that quote the forwarded message whole (Apple Mail) put it
        // one level down; that level becomes the forwarded text's own.
        const int base=first<lines.size()?quoteLevel(lines[first]).first:0;
        QList<std::pair<int,QString>> body;
        for(int i=first;i<lines.size();++i){const auto q=quoteLevel(lines[i]);body.append({std::max(0,q.first-base),q.second});}
        static const QRegularExpression field("^(From|To|Cc|Bcc|Date|Sent|Subject|Reply-To|Von|An|Datum|Gesendet|Betreff|De|Objet|Envoy\\x{e9}|\\x{c0}|Para|Fecha|Asunto|Enviado|Assunto|Da|Oggetto|Inviato|A)\\s*:\\s*(.*)$",QRegularExpression::CaseInsensitiveOption);
        static const QRegularExpression sender("^(From|Von|De|Da)$",QRegularExpression::CaseInsensitiveOption);
        QList<std::pair<QString,QString>> headers; int next=0;
        while(next<body.size() && body[next].first==0){const auto m=field.match(body[next].second.trimmed());if(!m.hasMatch())break;headers.append({m.captured(1),m.captured(2).trimmed()});++next;}
        if(headers.size()>=2 && sender.match(headers.first().first).hasMatch()) {
            QString table="<table cellspacing=\"0\" cellpadding=\"0\">";
            for(const auto &[label,value]:headers)table+="<tr><td style=\"padding:0 16px 4px 0\"><b>"+label.toHtmlEscaped()+"</b></td><td style=\"padding:0 0 4px 0\">"+value.toHtmlEscaped()+"</td></tr>";
            row(table+"</table>",QStringLiteral("headers"),0);
        } else next=0;
        struct Group { int level; QStringList lines; };
        QList<Group> groups;
        for(int i=next;i<body.size();++i) {
            const auto &[level,text]=body[i];
            const bool blank=text.trimmed().isEmpty();
            // A forward inside the forwarded text starts over at its own marker.
            if(level==0 && depth<32 && forwardMarker(text.trimmed())) {
                QStringList rest; for(int j=i;j<body.size();++j)rest.append(QString("> ").repeated(body[j].first)+body[j].second);
                for(const auto &group:std::as_const(groups))row(plainHtml(group.lines.join('\n'),group.level?12:24,group.level?0:-1),group.level?"quote":"",group.level);
                forward(rest,depth+1); return;
            }
            if(groups.isEmpty() || (!blank && groups.last().level!=level))groups.append(Group{blank?0:level,{}});
            groups.last().lines.append(text);
        }
        for(int g=0;g<groups.size();++g) {
            auto &group=groups[g];
            // An "On … wrote:" closing a group introduces the deeper quote after it.
            QString introduction;
            if(g+1<groups.size() && groups[g+1].level>group.level) {
                int end=group.lines.size(); while(end>0 && group.lines[end-1].trimmed().isEmpty()) --end;
                for(int start=end-1;start>=0 && start>=end-3 && !group.lines[start].trimmed().isEmpty();--start) {
                    const auto joined=group.lines.mid(start,end-start).join(' ').simplified();
                    if(attribution(joined)){introduction=joined;group.lines=group.lines.mid(0,start);break;}
                }
            }
            row(plainHtml(group.lines.join('\n'),group.level?12:24,group.level?0:-1),group.level?"quote":"",group.level);
            if(!introduction.isEmpty())row(plainHtml(introduction,0),QStringLiteral("attribution"),group.level);
        }
    };
    // Callers discard the returned list, so quoted history is never emitted;
    // what matters is `serial`, which counts reply boundaries for htmlFragment.
    auto quote=[&](QVariantList parents) {
        const int id=++serial;
        parents.append(QString::number(id)); return parents;
    };
    std::function<void(QString,QVariantList,int)> parsePlain;
    parsePlain=[&](QString body,QVariantList parents,int depth) {
        if (!parents.isEmpty()) return;
        body.replace("\r\n","\n");body.replace('\r','\n');
        if(depth>=32){append(body,parents,false);return;}
        const auto lines=body.split('\n'); QString pending;
        for(int i=0;i<lines.size();) {
            const auto line=lines[i].trimmed();
            if(line.startsWith('>')) {
                append(pending,parents,false);pending.clear();quote(parents);
                while(i<lines.size() && (lines[i].trimmed().startsWith('>') || lines[i].trimmed().isEmpty())) ++i;
                continue;
            }
            // A forwarded message is itself content the sender chose to send.
            if (forwardMarker(line)) { append(pending,parents,false); forward(lines.mid(i),depth); return; }
            if (line.contains("Forwarded message",Qt::CaseInsensitive)) {
                pending += lines.mid(i).join('\n'); append(pending,parents,false); return;
            }
            int headerEnd=-1;
            if(QRegularExpression("^-{2,}\\s*(Original Message)\\s*-{2,}$",QRegularExpression::CaseInsensitiveOption).match(line).hasMatch())headerEnd=i;
            if(line.startsWith("On ")) {
                QString joined;
                for(int j=i;j<qMin(i+5,int(lines.size()));++j){joined+=' '+lines[j].trimmed();if(joined.trimmed().endsWith("wrote:")){headerEnd=j;break;}}
            }
            if(line.startsWith("From:",Qt::CaseInsensitive)) {
                QSet<QString> fields;
                for(int j=i;j<qMin(i+9,int(lines.size()));++j){const auto m=QRegularExpression("^(From|To|Sent|Date|Subject|Cc):",QRegularExpression::CaseInsensitiveOption).match(lines[j].trimmed());if(m.hasMatch())fields.insert(m.captured(1).toLower());}
                if(fields.contains("to") && (fields.contains("date")||fields.contains("sent")) && fields.contains("subject")) {
                    headerEnd=i;
                    for(int j=i+1;j<qMin(i+9,int(lines.size()));++j){if(QRegularExpression("^(From|To|Sent|Date|Subject|Cc):",QRegularExpression::CaseInsensitiveOption).match(lines[j].trimmed()).hasMatch())headerEnd=j;else if(!lines[j].trimmed().isEmpty())break;}
                }
            }
            if(headerEnd>=0) {
                append(pending,parents,false);quote(parents);
                auto history=lines.mid(headerEnd+1);
                if (line.startsWith("On ") && std::any_of(history.begin(),history.end(),[](const QString &s){return s.trimmed().startsWith('>');})) {
                    // Inline answers below an attribution are still new text.
                    parsePlain(history.join('\n'),parents,depth+1); return;
                }
                return;
            }
            pending+=lines[i++]+'\n';
        }
        append(pending,parents,false);
    };
    const auto htmlFragment = [&](const QString &body,const QVariantList &parents,int depth) {
        if (!parents.isEmpty()) return;
        QTextDocument text; text.setHtml(sanitize(body));
        const auto before=rows.size(), quotes=qsizetype(serial);
        parsePlain(text.toPlainText(),parents,depth);
        // Preserve rich formatting unless a textual reply boundary requires a
        // trimmed contribution. This also catches attribution before a quote
        // container, not just at the end of the whole HTML message.
        if (serial == quotes) { while(rows.size()>before)rows.removeLast();append(body,parents,true); }
    };
    std::function<void(QString,QVariantList,int)> parseHtml;
    parseHtml=[&](QString body,QVariantList parents,int depth) {
        if (!parents.isEmpty()) return;
        if(depth>=32){append(body,parents,true);return;}
        // A tag scan, not a DOM: find each quote container's matching close
        // tag by counting nested tags of the same name, emit the text before
        // it, and skip the container. Outlook's divRplyFwdMsg ends the reply.
        static const QRegularExpression tag("<\\s*(/?)\\s*([a-zA-Z0-9]+)\\b[^>]*>");
        auto tokens=tag.globalMatch(body);qsizetype consumed=0;
        while(tokens.hasNext()) {
            const auto open=tokens.next();if(open.capturedStart()<consumed || !open.captured(1).isEmpty())continue;
            const auto name=open.captured(2).toLower(), token=open.captured();
            const bool tail=QRegularExpression("\\bid\\s*=\\s*[\"']divRplyFwdMsg[\"']",QRegularExpression::CaseInsensitiveOption).match(token).hasMatch();
            const bool explicitQuote=name=="blockquote" || ((name=="div"||name=="section") && QRegularExpression("\\bclass\\s*=\\s*[\"'][^\"']*\\b(gmail_quote|yahoo_quoted|protonmail_quote)\\b",QRegularExpression::CaseInsensitiveOption).match(token).hasMatch());
            if(!tail && !explicitQuote)continue;
            int level=1;qsizetype closeEnd=body.size();
            auto remaining=tag.globalMatch(body,open.capturedEnd());
            while(remaining.hasNext()){const auto t=remaining.next();if(t.captured(2).compare(name,Qt::CaseInsensitive)!=0)continue;level+=t.captured(1).isEmpty()?1:-1;if(!level){closeEnd=t.capturedEnd();break;}}
            htmlFragment(body.mid(consumed,open.capturedStart()-consumed),parents,depth);
            quote(parents);
            if(tail)return;
            consumed=closeEnd;
        }
        const auto rest=body.mid(consumed);
        // Outlook/plain reply headers may be rendered as ordinary HTML paragraphs.
        htmlFragment(rest,parents,depth);
    };
    if(html.isEmpty())parsePlain(plain,{},0);
    else {
        QString body=html;
        body.remove(QRegularExpression("<(head|style|script)\\b[^>]*>[\\s\\S]*?</\\1\\s*>",QRegularExpression::CaseInsensitiveOption));
        // Forwarded content is intentionally included, including clients that
        // wrap it in the same container they use for ordinary reply history.
        if (body.contains("forwarded message", Qt::CaseInsensitive)) { append(body,{},true); return rows; }
        parseHtml(body,{},0);
    }
    return rows;
}
}
