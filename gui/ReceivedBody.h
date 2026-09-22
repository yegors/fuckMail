#pragma once
#include <QRegularExpression>
#include <QSet>
#include <QTextDocument>
#include <QUrl>
#include <QVariantList>
#include <functional>
#include <algorithm>

// Display-only projection. Source bytes and authored/composer text never change.
namespace ReceivedBody {
inline QString plainHtml(QString text) {
    text.replace("\r\n","\n"); text.replace('\r','\n');
    const auto paragraphs=text.trimmed().split(QRegularExpression("\\n[\\t ]*\\n(?:[\\t ]*\\n)*"));
    QString html;
    for(const auto &p:paragraphs) {
        QString rendered;qsizetype end=0;
        auto links=QRegularExpression("https?://[^\\s<>]+",QRegularExpression::CaseInsensitiveOption).globalMatch(p);
        while(links.hasNext()) {
            const auto link=links.next();rendered+=p.mid(end,link.capturedStart()-end).toHtmlEscaped();
            const auto escaped=link.captured().toHtmlEscaped();const QUrl url(link.captured(),QUrl::StrictMode);
            rendered+=url.isValid()?"<a href=\""+escaped+"\">"+escaped+"</a>":escaped;end=link.capturedEnd();
        }
        rendered+=p.mid(end).toHtmlEscaped();
        html += "<p style=\"white-space:pre-wrap;margin-top:0;margin-bottom:7px;line-height:145%\">"+rendered.replace('\n',"<br>")+"</p>";
    }
    return html;
}
inline QVariantList sections(const QString &html,const QString &plain,const std::function<QString(const QString &)> &sanitize) {
    QVariantList rows; int serial=0;
    auto append=[&](const QString &body,const QVariantList &parents,bool rich) {
        if(!parents.isEmpty() || body.trimmed().isEmpty())return;
        const auto rendered=rich?sanitize(body):plainHtml(body);
        QTextDocument check; check.setHtml(rendered);
        if(!check.toPlainText().trimmed().isEmpty())rows.append(QVariantMap{{"html",rendered},{"parents",parents},{"quote",false}});
    };
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
            if (line.contains("Forwarded message",Qt::CaseInsensitive) || line.compare("Begin forwarded message:",Qt::CaseInsensitive)==0) {
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
