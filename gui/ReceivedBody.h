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
        if(body.trimmed().isEmpty())return;
        const auto rendered=rich?sanitize(body):plainHtml(body);
        QTextDocument check; check.setHtml(rendered);
        if(!check.toPlainText().trimmed().isEmpty())rows.append(QVariantMap{{"html",rendered},{"parents",parents},{"quote",false}});
    };
    auto quote=[&](QVariantList parents) {
        const int id=++serial;
        rows.append(QVariantMap{{"quote",true},{"key",QString::number(id)},{"parents",parents}});
        parents.append(QString::number(id)); return parents;
    };
    std::function<void(QString,QVariantList,int)> parsePlain;
    parsePlain=[&](QString body,QVariantList parents,int depth) {
        body.replace("\r\n","\n");body.replace('\r','\n');
        if(depth>=32){append(body,parents,false);return;}
        const auto lines=body.split('\n'); QString pending;
        for(int i=0;i<lines.size();) {
            const auto line=lines[i].trimmed();
            if(line.startsWith('>')) {
                append(pending,parents,false);pending.clear();QString quoted;
                while(i<lines.size() && (lines[i].trimmed().startsWith('>') || lines[i].trimmed().isEmpty())) {
                    QString part=lines[i++];part.remove(QRegularExpression("^\\s*> ?"));quoted+=part+'\n';
                }
                parsePlain(quoted,quote(parents),depth+1);continue;
            }
            int headerEnd=-1;
            if(QRegularExpression("^-{2,}\\s*(Original Message|Forwarded message)\\s*-{2,}$",QRegularExpression::CaseInsensitiveOption).match(line).hasMatch())headerEnd=i;
            if(line.startsWith("On ")) {
                QString joined;
                for(int j=i;j<qMin(i+5,int(lines.size()));++j){joined+=' '+lines[j].trimmed();if(joined.trimmed().endsWith("wrote:") && joined.contains('@')){headerEnd=j;break;}}
            }
            if(line.startsWith("From:",Qt::CaseInsensitive)) {
                QSet<QString> fields;
                for(int j=i;j<qMin(i+9,int(lines.size()));++j){const auto m=QRegularExpression("^(From|To|Sent|Date|Subject|Cc):",QRegularExpression::CaseInsensitiveOption).match(lines[j].trimmed());if(m.hasMatch())fields.insert(m.captured(1).toLower());}
                if(fields.contains("to") && (fields.contains("date")||fields.contains("sent")) && fields.contains("subject")) {
                    headerEnd=i;
                    for(int j=i+1;j<qMin(i+9,int(lines.size()));++j){if(QRegularExpression("^(From|To|Sent|Date|Subject|Cc):",QRegularExpression::CaseInsensitiveOption).match(lines[j].trimmed()).hasMatch())headerEnd=j;else if(!lines[j].trimmed().isEmpty())break;}
                }
            }
            if(headerEnd>=0 && headerEnd+1<lines.size()) {
                if(line.startsWith("--")) {
                    for(int j=headerEnd+1;j<qMin(i+12,int(lines.size()));++j) {
                        if(lines[j].trimmed().isEmpty())continue;
                        if(QRegularExpression("^(From|To|Sent|Date|Subject|Cc|Bcc):",QRegularExpression::CaseInsensitiveOption).match(lines[j].trimmed()).hasMatch())headerEnd=j;
                        else break;
                    }
                }
                append(pending,parents,false);const auto child=quote(parents);
                append(lines.mid(i,headerEnd-i+1).join('\n'),child,false);
                auto history=lines.mid(headerEnd+1);
                const bool prefixed=std::all_of(history.begin(),history.end(),[](const QString &s){return s.trimmed().isEmpty() || s.trimmed().startsWith('>');});
                if(prefixed)for(auto &s:history)s.remove(QRegularExpression("^\\s*> ?"));
                parsePlain(history.join('\n'),child,depth+1);return;
            }
            pending+=lines[i++]+'\n';
        }
        append(pending,parents,false);
    };
    std::function<void(QString,QVariantList,int)> parseHtml;
    parseHtml=[&](QString body,QVariantList parents,int depth) {
        if(depth>=32){append(body,parents,true);return;}
        static const QRegularExpression tag("<\\s*(/?)\\s*([a-zA-Z0-9]+)\\b[^>]*>");
        auto tokens=tag.globalMatch(body);qsizetype consumed=0;
        while(tokens.hasNext()) {
            const auto open=tokens.next();if(open.capturedStart()<consumed || !open.captured(1).isEmpty())continue;
            const auto name=open.captured(2).toLower(), token=open.captured();
            const bool tail=QRegularExpression("\\bid\\s*=\\s*[\"']divRplyFwdMsg[\"']",QRegularExpression::CaseInsensitiveOption).match(token).hasMatch();
            const bool explicitQuote=name=="blockquote" || ((name=="div"||name=="section") && QRegularExpression("\\bclass\\s*=\\s*[\"'][^\"']*\\b(gmail_quote|yahoo_quoted|protonmail_quote)\\b",QRegularExpression::CaseInsensitiveOption).match(token).hasMatch());
            if(!tail && !explicitQuote)continue;
            int level=1;qsizetype closeStart=body.size(),closeEnd=body.size();
            auto remaining=tag.globalMatch(body,open.capturedEnd());
            while(remaining.hasNext()){const auto t=remaining.next();if(t.captured(2).compare(name,Qt::CaseInsensitive)!=0)continue;level+=t.captured(1).isEmpty()?1:-1;if(!level){closeStart=t.capturedStart();closeEnd=t.capturedEnd();break;}}
            append(body.mid(consumed,open.capturedStart()-consumed),parents,true);
            const auto child=quote(parents);
            if(tail){parseHtml(body.mid(open.capturedEnd()),child,depth+1);return;}
            auto inner=body.mid(open.capturedEnd(),closeStart-open.capturedEnd());
            // Gmail's attribution wrapper and its immediate blockquote describe
            // one reply, not two disclosure levels. Keep real nested quotes.
            if(name!="blockquote") {
                const auto gmail=QRegularExpression("^\\s*(<div\\b[^>]*\\bclass\\s*=\\s*[\"'][^\"']*\\bgmail_attr\\b[^\"']*[\"'][^>]*>[\\s\\S]*?</div>)\\s*<blockquote\\b[^>]*>([\\s\\S]*)</blockquote>\\s*$",QRegularExpression::CaseInsensitiveOption).match(inner);
                if(gmail.hasMatch()){append(gmail.captured(1),child,true);inner=gmail.captured(2);}
            }
            parseHtml(inner,child,depth+1);
            consumed=closeEnd;
        }
        const auto rest=body.mid(consumed);
        // Outlook/plain reply headers may be rendered as ordinary HTML paragraphs.
        QTextDocument text; text.setHtml(sanitize(rest));
        const auto before=rows.size();parsePlain(text.toPlainText(),parents,depth);
        bool hasQuote=false;for(auto i=before;i<rows.size();++i)hasQuote|=rows[i].toMap().value("quote").toBool();
        if(!hasQuote){while(rows.size()>before)rows.removeLast();append(rest,parents,true);}
    };
    if(html.isEmpty())parsePlain(plain,{},0);
    else {
        QString body=html;
        body.remove(QRegularExpression("<(head|style|script)\\b[^>]*>[\\s\\S]*?</\\1\\s*>",QRegularExpression::CaseInsensitiveOption));
        parseHtml(body,{},0);
    }
    return rows;
}
}
